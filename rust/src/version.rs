//! Versão do app e comparação semântica (porte de `OBSVersion.pas`).
//!
//! A versão vem do `git describe --tags --always --dirty`, lido pelo
//! `build.rs` (o `gen-version.bat` do Delphi):
//! * `v0.15.0-beta` — release: exatamente na tag, limpo;
//! * `v0.15.0-beta-1-gd7e4f73` — commits depois da tag;
//! * `...-dirty` — alterações não commitadas;
//! * `d7e4f73` — repositório sem tags: não é versão.
//!
//! Os três últimos são build de desenvolvimento: a checagem de atualização
//! NÃO roda neles (estão à frente da tag por definição; avisaria sempre).

/// Versão crua do `git describe`; `dev` sem git.
pub const APP_VERSION: &str = match option_env!("NOOBS_APP_VERSION") {
    Some(v) => v,
    None => "dev",
};

#[derive(Debug, Default, PartialEq, Eq)]
struct SemVer {
    major: u32,
    minor: u32,
    patch: u32,
    /// Vazio = release final.
    pre: String,
}

/// Segmento `<n>-g<hash>` (commits depois da tag) nas posições `i`, `i+1`.
fn is_describe_pair(parts: &[&str], i: usize) -> bool {
    i + 1 < parts.len()
        && parts[i].parse::<u32>().is_ok()
        && parts[i + 1].len() >= 2
        && parts[i + 1].starts_with('g')
}

/// Sufixo do describe: `-<n>-g<hash>` e/ou `-dirty`.
fn has_describe_suffix(s: &str) -> bool {
    if s.ends_with("-dirty") {
        return true;
    }
    let parts: Vec<&str> = s.split('-').collect();
    (0..parts.len()).any(|i| i >= 1 && is_describe_pair(&parts, i))
}

/// Aceita `v1.2.3`, `1.2.3`, `1.2.3-beta`; ignora o sufixo do describe.
fn parse_semver(s: &str) -> Option<SemVer> {
    let t = s.trim();
    let t = t.strip_prefix('v').or_else(|| t.strip_prefix('V')).unwrap_or(t);
    let t = t.strip_suffix("-dirty").unwrap_or(t);
    if t.is_empty() {
        return None;
    }
    let parts: Vec<&str> = t.split('-').collect();
    let mut pre: Vec<&str> = Vec::new();
    let mut i = 1;
    while i < parts.len() {
        // Daqui pra frente é sufixo do describe.
        if is_describe_pair(&parts, i) {
            break;
        }
        pre.push(parts[i]);
        i += 1;
    }
    let core: Vec<&str> = parts[0].split('.').collect();
    // Exige ao menos MAJOR.MINOR.
    if core.len() < 2 {
        return None;
    }
    Some(SemVer {
        major: core[0].parse().ok()?,
        minor: core[1].parse().ok()?,
        patch: if core.len() >= 3 { core[2].parse().ok()? } else { 0 },
        pre: pre.join("-"),
    })
}

/// O build NÃO está exatamente numa tag limpa (ou não há versão).
pub fn is_dev_build() -> bool {
    parse_semver(APP_VERSION).is_none() || has_describe_suffix(APP_VERSION)
}

/// Para a UI: sem o `v`; build de dev mantém o sufixo de propósito.
pub fn display_version() -> String {
    APP_VERSION.strip_prefix('v').unwrap_or(APP_VERSION).to_owned()
}

/// -1/0/1. MAJOR.MINOR.PATCH numérico e pré-lançamento pela regra semver
/// (`1.0.0-beta < 1.0.0`); versão inválida dos dois lados = 0.
pub fn compare_versions(a: &str, b: &str) -> i32 {
    let (Some(va), Some(vb)) = (parse_semver(a), parse_semver(b)) else { return 0 };
    let ord = (va.major, va.minor, va.patch).cmp(&(vb.major, vb.minor, vb.patch)).then_with(|| {
        match (va.pre.is_empty(), vb.pre.is_empty()) {
            (true, true) => std::cmp::Ordering::Equal,
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            (false, false) => va.pre.to_lowercase().cmp(&vb.pre.to_lowercase()),
        }
    });
    ord as i32
}

/// `remote` é mais novo que a versão atual? Qualquer dúvida = `false`:
/// avisar à toa destrói a confiança no aviso.
pub fn is_remote_newer(remote: &str) -> bool {
    if remote.trim().is_empty() || is_dev_build() || parse_semver(remote).is_none() {
        return false;
    }
    compare_versions(APP_VERSION, remote) < 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sufixo_do_describe() {
        assert!(!has_describe_suffix("v0.15.0-beta"));
        assert!(has_describe_suffix("v0.15.0-beta-1-gd7e4f73"));
        assert!(has_describe_suffix("v0.15.0-beta-dirty"));
        assert!(parse_semver("d7e4f73").is_none());
    }

    #[test]
    fn pre_lancamento_e_menor() {
        assert_eq!(compare_versions("v1.0.0-beta", "1.0.0"), -1);
        assert_eq!(compare_versions("v0.16.0-beta", "v0.15.2-beta"), 1);
        assert_eq!(compare_versions("v0.15.0-beta-3-gabc1234", "v0.15.0-beta"), 0);
        assert_eq!(compare_versions("1.2", "1.2.0"), 0);
    }
}
