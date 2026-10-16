//! Cliente HTTP mínimo sobre o WinHTTP, compartilhado pela transcrição (servidor
//! remoto e servidor local) e pelo download do motor local.
//!
//! Existe em vez de uma crate HTTP por dois motivos que vieram do Delphi:
//! * o `THTTPClient` não ajusta a ESPERA PELA RESPOSTA depois do envio
//!   (`WINHTTP_OPTION_RECEIVE_RESPONSE_TIMEOUT`, que o Windows fixa em 90 s) — um
//!   bloco lento do modelo local virava "Error sending data (12002)" (pegadinha #60t);
//! * o cancelamento precisa fechar o handle do pedido em voo DE OUTRA THREAD, para a
//!   chamada síncrona voltar com erro na hora ([`AbortSlot`]).
//!
//! Segue redirecionamentos sozinho (GitHub e HuggingFace respondem 302 para a CDN) e
//! usa o proxy do sistema, exceto quando `proxy = false` (127.0.0.1).

use std::ffi::c_void;
use std::sync::Mutex;

use windows::core::PCWSTR;
use windows::Win32::Networking::WinHttp::{
    WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest, WinHttpQueryDataAvailable, WinHttpQueryHeaders, WinHttpReadData,
    WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetOption, WinHttpSetTimeouts, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
    WINHTTP_ACCESS_TYPE_NO_PROXY, WINHTTP_FLAG_SECURE, WINHTTP_OPEN_REQUEST_FLAGS, WINHTTP_OPTION_RECEIVE_RESPONSE_TIMEOUT,
    WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE,
};

use crate::win::wide;

#[derive(Debug, Clone, Copy)]
pub struct Timeouts {
    pub connect_ms: i32,
    pub send_ms: i32,
    /// Espera pela resposta depois do envio (e entre pedaços do corpo).
    pub receive_ms: i32,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self { connect_ms: 15_000, send_ms: 30_000, receive_ms: 60_000 }
    }
}

#[derive(Debug)]
pub struct Response {
    pub status: u32,
    pub body: Vec<u8>,
}

impl Response {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Falha de transporte (nem chegou resposta): conexão recusada, tempo esgotado,
/// cancelamento. `code` = erro do WinHTTP (12002 = tempo esgotado), 0 = abortado.
#[derive(Debug, Clone)]
pub struct HttpError {
    pub stage: &'static str,
    pub code: u32,
    /// Limite que estourou, para a mensagem (só vale com 12002).
    pub limit_ms: i32,
}

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.code == 12002 {
            write!(f, "{}: tempo esgotado ({} s, WinHTTP 12002)", self.stage, self.limit_ms / 1000)
        } else {
            write!(f, "{}: WinHTTP {}", self.stage, self.code)
        }
    }
}

/// Handle do pedido em voo, visível a outra thread para o cancelamento. Quem fecha é
/// UM só — o dono ao terminar ou o `abort` —, decidido sob o lock (fechar duas vezes
/// seria handle inválido).
#[derive(Default)]
pub struct AbortSlot(Mutex<Option<usize>>);

impl AbortSlot {
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    /// Fecha o pedido em voo, se houver. `true` = havia.
    pub fn abort(&self) -> bool {
        let mut g = self.0.lock().unwrap();
        match g.take() {
            Some(h) => {
                unsafe {
                    let _ = WinHttpCloseHandle(h as *mut c_void);
                }
                true
            }
            None => false,
        }
    }
}

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = WinHttpCloseHandle(self.0);
            }
        }
    }
}

/// O pedido, registrado no [`AbortSlot`] enquanto vive.
struct ReqGuard<'a> {
    h: *mut c_void,
    slot: Option<&'a AbortSlot>,
}

impl Drop for ReqGuard<'_> {
    fn drop(&mut self) {
        match self.slot {
            Some(slot) => {
                let mut g = slot.0.lock().unwrap();
                // Se o abort já fechou o handle ele tirou o registro: fechar de novo
                // seria handle inválido.
                if *g == Some(self.h as usize) {
                    *g = None;
                    unsafe {
                        let _ = WinHttpCloseHandle(self.h);
                    }
                }
            }
            None => unsafe {
                let _ = WinHttpCloseHandle(self.h);
            },
        }
    }
}

pub struct Request<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub headers: Vec<(String, String)>,
    pub body: &'a [u8],
    pub timeouts: Timeouts,
    /// Usa o proxy do sistema. `false` para 127.0.0.1.
    pub proxy: bool,
    pub abort: Option<&'a AbortSlot>,
}

impl<'a> Request<'a> {
    pub fn new(method: &'a str, url: &'a str) -> Self {
        Self { method, url, headers: Vec::new(), body: &[], timeouts: Timeouts::default(), proxy: true, abort: None }
    }

    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    pub fn body(mut self, body: &'a [u8]) -> Self {
        self.body = body;
        self
    }

    pub fn timeouts(mut self, t: Timeouts) -> Self {
        self.timeouts = t;
        self
    }

    pub fn no_proxy(mut self) -> Self {
        self.proxy = false;
        self
    }

    pub fn abort_slot(mut self, slot: &'a AbortSlot) -> Self {
        self.abort = Some(slot);
        self
    }
}

struct Parsed {
    secure: bool,
    host: String,
    port: u16,
    path: String,
}

fn parse_url(url: &str) -> Option<Parsed> {
    let (secure, rest) = if let Some(r) = url.strip_prefix("https://") {
        (true, r)
    } else if let Some(r) = url.strip_prefix("http://") {
        (false, r)
    } else {
        return None;
    };
    let (authority, path) = match rest.find(['/', '?']) {
        Some(i) if rest.as_bytes()[i] == b'/' => (&rest[..i], rest[i..].to_owned()),
        Some(i) => (&rest[..i], format!("/{}", &rest[i..])),
        None => (rest, "/".to_owned()),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(']') || h.starts_with('[') => (h.to_owned(), p.parse::<u16>().ok()?),
        _ => (authority.to_owned(), if secure { 443 } else { 80 }),
    };
    (!host.is_empty()).then_some(Parsed { secure, host, port, path })
}

fn fail(stage: &'static str, e: &windows::core::Error, limit_ms: i32) -> HttpError {
    // HRESULT_FROM_WIN32: o código do WinHTTP fica nos 16 bits de baixo.
    HttpError { stage, code: (e.code().0 as u32) & 0xFFFF, limit_ms }
}

/// Envia o pedido e recebe os cabeçalhos; `on_body` é chamado com cada pedaço do corpo
/// (`false` = abortar). Devolve o status HTTP.
fn run(req: &Request, on_body: &mut dyn FnMut(u32, &[u8]) -> bool) -> Result<u32, HttpError> {
    let p = parse_url(req.url).ok_or(HttpError { stage: "URL", code: 0, limit_ms: 0 })?;
    unsafe {
        let access = if req.proxy { WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY } else { WINHTTP_ACCESS_TYPE_NO_PROXY };
        let agent = wide("NoOBS");
        let session = Handle(WinHttpOpen(PCWSTR(agent.as_ptr()), access, PCWSTR::null(), PCWSTR::null(), 0));
        if session.0.is_null() {
            return Err(HttpError { stage: "WinHttpOpen", code: windows::Win32::Foundation::GetLastError().0, limit_ms: 0 });
        }
        let t = req.timeouts;
        // Resolução curta; a CONEXÃO acontece dentro do WinHttpSendRequest, e é ali que
        // estoura um servidor vivo que parou de aceitar pedidos.
        let _ = WinHttpSetTimeouts(session.0, 5000, t.connect_ms, t.send_ms, t.receive_ms);
        let host = wide(&p.host);
        let conn = Handle(WinHttpConnect(session.0, PCWSTR(host.as_ptr()), p.port, 0));
        if conn.0.is_null() {
            return Err(HttpError { stage: "WinHttpConnect", code: windows::Win32::Foundation::GetLastError().0, limit_ms: t.connect_ms });
        }
        let (verb, path) = (wide(req.method), wide(&p.path));
        let flags = if p.secure { WINHTTP_FLAG_SECURE } else { WINHTTP_OPEN_REQUEST_FLAGS(0) };
        let h = WinHttpOpenRequest(conn.0, PCWSTR(verb.as_ptr()), PCWSTR(path.as_ptr()), PCWSTR::null(), PCWSTR::null(), std::ptr::null(), flags);
        if h.is_null() {
            return Err(HttpError { stage: "WinHttpOpenRequest", code: windows::Win32::Foundation::GetLastError().0, limit_ms: t.connect_ms });
        }
        if let Some(slot) = req.abort {
            *slot.0.lock().unwrap() = Some(h as usize);
        }
        let _req = ReqGuard { h, slot: req.abort };
        let timeout = t.receive_ms as u32;
        let _ = WinHttpSetOption(Some(h as *const c_void), WINHTTP_OPTION_RECEIVE_RESPONSE_TIMEOUT, Some(&timeout.to_le_bytes()));

        let headers: String = req.headers.iter().map(|(k, v)| format!("{k}: {v}\r\n")).collect();
        let headers_w: Vec<u16> = headers.encode_utf16().collect();
        let len = req.body.len() as u32;
        let optional = if req.body.is_empty() { None } else { Some(req.body.as_ptr() as *const c_void) };
        let aborted = || HttpError { stage: "abort", code: 0, limit_ms: 0 };
        let stage_err = |stage: &'static str, e: windows::core::Error, limit: i32| -> HttpError {
            // Handle fechado por `AbortSlot::abort`: a falha é o cancelamento.
            if let Some(slot) = req.abort {
                if slot.0.lock().unwrap().is_none() {
                    return aborted();
                }
            }
            fail(stage, &e, limit)
        };
        WinHttpSendRequest(h, if headers_w.is_empty() { None } else { Some(&headers_w) }, optional, len, len, 0)
            .map_err(|e| stage_err("WinHttpSendRequest", e, t.send_ms))?;
        WinHttpReceiveResponse(h, std::ptr::null_mut()).map_err(|e| stage_err("WinHttpReceiveResponse", e, t.receive_ms))?;

        let mut code = 0u32;
        let mut code_len = std::mem::size_of::<u32>() as u32;
        let status = if WinHttpQueryHeaders(
            h,
            WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
            PCWSTR::null(),
            Some(&mut code as *mut u32 as *mut c_void),
            &mut code_len,
            std::ptr::null_mut(),
        )
        .is_ok()
        {
            code
        } else {
            0
        };
        let mut buf = Vec::new();
        loop {
            let mut avail = 0u32;
            WinHttpQueryDataAvailable(h, &mut avail).map_err(|e| stage_err("WinHttpQueryDataAvailable", e, t.receive_ms))?;
            if avail == 0 {
                break;
            }
            buf.resize(avail as usize, 0);
            let mut got = 0u32;
            WinHttpReadData(h, buf.as_mut_ptr() as *mut c_void, avail, &mut got).map_err(|e| stage_err("WinHttpReadData", e, t.receive_ms))?;
            if got == 0 {
                break;
            }
            if !on_body(status, &buf[..got as usize]) {
                return Err(aborted());
            }
        }
        Ok(status)
    }
}

/// Pedido completo: devolve status e corpo inteiro.
pub fn request(req: &Request) -> Result<Response, HttpError> {
    let mut body = Vec::new();
    let status = run(req, &mut |_, chunk| {
        body.extend_from_slice(chunk);
        true
    })?;
    Ok(Response { status, body })
}

/// Pedido com o corpo entregue em pedaços (download grande). `sink` recebe o status
/// HTTP (já conhecido quando o primeiro pedaço chega — dá para não gravar corpo de
/// erro) e devolve `false` para abortar. Devolve o status HTTP.
pub fn stream(req: &Request, sink: &mut dyn FnMut(u32, &[u8]) -> bool) -> Result<u32, HttpError> {
    run(req, sink)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_com_porta_e_caminho() {
        let p = parse_url("http://127.0.0.1:8000/jobs?x=1").unwrap();
        assert_eq!((p.secure, p.host.as_str(), p.port, p.path.as_str()), (false, "127.0.0.1", 8000, "/jobs?x=1"));
    }

    #[test]
    fn url_https_sem_caminho() {
        let p = parse_url("https://huggingface.co").unwrap();
        assert_eq!((p.secure, p.port, p.path.as_str()), (true, 443, "/"));
    }

    #[test]
    fn url_com_query_sem_barra() {
        let p = parse_url("https://drive.usercontent.google.com?id=1").unwrap();
        assert_eq!(p.path, "/?id=1");
    }

    #[test]
    fn url_invalida() {
        assert!(parse_url("ftp://x").is_none());
    }
}
