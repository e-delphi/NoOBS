//! Webcams via DirectShow, com friendly name (porte de `WinWebcam.pas`).
//! DirectShow não tem callback de hot-plug: quem reage é o `WM_DEVICECHANGE`
//! do Bridge, que re-enumera por aqui.

use windows::core::{BSTR, PCWSTR};
use windows::Win32::Media::DirectShow::ICreateDevEnum;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CreateBindCtx, IMoniker, CLSCTX_INPROC_SERVER,
    COINIT_MULTITHREADED,
};
use windows::Win32::System::Com::StructuredStorage::IPropertyBag;
use windows::Win32::System::Variant::VARIANT;

use crate::log;
use crate::types::Webcam;

// CLSIDs do DirectShow (Win32 não os exporta como constantes na `windows`).
const CLSID_SYSTEM_DEVICE_ENUM: windows::core::GUID = windows::core::GUID::from_u128(0x62BE5D10_60EB_11d0_BD3B_00A0C911CE86);
const CLSID_VIDEO_INPUT_DEVICE_CATEGORY: windows::core::GUID =
    windows::core::GUID::from_u128(0x860BB310_5D01_11d0_BD3B_00A0C911CE86);

fn moniker_display_name(m: &IMoniker) -> String {
    unsafe {
        let Ok(bind) = CreateBindCtx(0) else { return String::new() };
        let Ok(p) = m.GetDisplayName(&bind, None) else { return String::new() };
        if p.is_null() {
            return String::new();
        }
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

/// Quando o `IPropertyBag` falha (alguns drivers), monta um nome legível a
/// partir do moniker (`vid_046d` → Logitech; não mapeamos o pid). Pura.
/// Fallback: "Webcam N".
pub fn friendly_from_moniker(device_id: &str, index: usize) -> String {
    const VENDORS: [(&str, &str); 8] = [
        ("vid_046d", "Logitech"),
        ("vid_045e", "Microsoft"),
        ("vid_05ac", "Apple"),
        ("vid_1bcf", "Sunplus"),
        ("vid_174f", "Syntek"),
        ("vid_04f2", "Chicony"),
        ("vid_0c45", "SONiX"),
        ("vid_13d3", "IMC"),
    ];
    let lower = device_id.to_lowercase();
    for (vid, name) in VENDORS {
        if lower.contains(vid) {
            let mut r = format!("{name} Webcam");
            if index > 0 {
                r.push_str(&format!(" ({})", index + 1));
            }
            return r;
        }
    }
    format!("Webcam {}", index + 1)
}

/// Tenta o `IPropertyBag` por várias chaves; vazio se nada funcionar (o
/// chamador cai em `friendly_from_moniker`).
fn read_friendly_name(m: &IMoniker) -> String {
    const KEYS: [&str; 3] = ["FriendlyName", "Description", "Name"];
    unsafe {
        let Ok(bind) = CreateBindCtx(0) else { return String::new() };
        let Ok(bag) = m.BindToStorage::<_, _, IPropertyBag>(&bind, None) else { return String::new() };
        for key in KEYS {
            let wide: Vec<u16> = key.encode_utf16().chain(std::iter::once(0)).collect();
            let mut v = VARIANT::default();
            if bag.Read(PCWSTR(wide.as_ptr()), &mut v, None).is_ok() {
                if let Ok(s) = BSTR::try_from(&v) {
                    let s = s.to_string();
                    if !s.is_empty() {
                        return s;
                    }
                }
            }
        }
    }
    String::new()
}

pub fn enumerate_webcams() -> Vec<Webcam> {
    let mut result: Vec<Webcam> = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let Ok(dev_enum) = CoCreateInstance::<_, ICreateDevEnum>(&CLSID_SYSTEM_DEVICE_ENUM, None, CLSCTX_INPROC_SERVER)
        else {
            return result;
        };
        let mut enum_mon = None;
        if dev_enum.CreateClassEnumerator(&CLSID_VIDEO_INPUT_DEVICE_CATEGORY, &mut enum_mon, 0).is_err() {
            return result;
        }
        let Some(enum_mon) = enum_mon else { return result };

        loop {
            let mut monikers: [Option<IMoniker>; 1] = [None];
            let mut fetched = 0u32;
            // S_OK (0) = entregou um; S_FALSE = acabou.
            if enum_mon.Next(&mut monikers, Some(&mut fetched)).0 != 0 {
                break;
            }
            let Some(moniker) = monikers[0].take() else { break };
            let display = moniker_display_name(&moniker);
            log!("Webcam enum: Display=\"{display}\"");
            // Filtra webcams "software" (OBS Virtual Camera, NVIDIA Broadcast,
            // Streamlabs…): hardware começa com "@device:pnp:".
            if display.starts_with("@device:sw:") {
                log!("Webcam enum: pulando software device.");
                continue;
            }
            let mut name = read_friendly_name(&moniker);
            if name.is_empty() {
                name = friendly_from_moniker(&display, result.len());
            }
            // O dshow_input do OBS espera `<friendly_name>:<device_path>`
            // (DecodeDeviceId splita no primeiro ':'); path = moniker sem o
            // prefixo "@device:pnp:".
            let path = display.strip_prefix("@device:pnp:").unwrap_or(&display).to_owned();
            let device_id = format!("{name}:{path}");
            log!("Webcam enum: Name=\"{name}\" DeviceId=\"{device_id}\"");
            if !path.is_empty() {
                result.push(Webcam { device_id, name, width: 1280, height: 720 });
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nome_pelo_fabricante() {
        assert_eq!(friendly_from_moniker("@device:pnp:\\\\?\\usb#vid_046d&pid_085e", 0), "Logitech Webcam");
        assert_eq!(friendly_from_moniker("usb#vid_046d", 1), "Logitech Webcam (2)");
    }

    #[test]
    fn nome_generico() {
        assert_eq!(friendly_from_moniker("usb#vid_ffff", 2), "Webcam 3");
    }
}
