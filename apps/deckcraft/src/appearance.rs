//! Linux appearance from the standard Settings portal. Native egui reports are used elsewhere.
//!
//! The portal uses numeric values: 0 = no preference, 1 = dark, 2 = light. Missing reports
//! leave resolution to the UI's documented Light fallback. There is no desktop polling.

#[cfg(target_os = "linux")]
const NAMESPACE: &str = "org.freedesktop.appearance";
#[cfg(target_os = "linux")]
const KEY: &str = "color-scheme";

#[cfg(any(target_os = "linux", test))]
fn decode(code: u32) -> Option<egui::Theme> {
    match code {
        1 => Some(egui::Theme::Dark),
        2 => Some(egui::Theme::Light),
        _ => None,
    }
}

#[cfg(target_os = "linux")]
fn portal_code(value: zbus::zvariant::OwnedValue) -> Option<u32> {
    let mut value: zbus::zvariant::Value<'_> = value.into();
    // Settings.Read returns a variant; tolerate bounded nested variants from portal backends.
    for _ in 0..4 {
        match value {
            zbus::zvariant::Value::Value(inner) => value = *inner,
            other => return u32::try_from(other).ok(),
        }
    }
    None
}

/// Subscribe before the initial read, publish every changed result, and wake an idle UI.
/// A slow or unavailable portal never blocks startup for more than 250 ms.
pub fn service(ctx: egui::Context) -> Option<deckcraft_ui_egui::SystemThemeFn> {
    #[cfg(target_os = "linux")]
    {
        use std::sync::{
            Arc,
            atomic::{AtomicU8, Ordering},
        };
        let value = Arc::new(AtomicU8::new(0));
        let worker_value = Arc::clone(&value);
        let (ready, wait) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("appearance-portal".into())
            .spawn(move || {
                let run = || -> Result<(), zbus::Error> {
                    let conn = zbus::blocking::connection::Builder::session()?.method_timeout(std::time::Duration::from_secs(2)).build()?;
                    let proxy = zbus::blocking::Proxy::new(
                        &conn,
                        "org.freedesktop.portal.Desktop",
                        "/org/freedesktop/portal/desktop",
                        "org.freedesktop.portal.Settings",
                    )?;
                    let signals = proxy.receive_signal("SettingChanged")?;
                    let publish = |code: Option<u32>| {
                        let next = match code.and_then(decode) {
                            Some(egui::Theme::Dark) => 1,
                            Some(egui::Theme::Light) => 2,
                            None => 0,
                        };
                        if worker_value.swap(next, Ordering::Relaxed) != next {
                            ctx.request_repaint_of(egui::ViewportId::ROOT);
                        }
                    };
                    let initial = proxy.call::<_, _, zbus::zvariant::OwnedValue>("Read", &(NAMESPACE, KEY)).ok().and_then(portal_code);
                    publish(initial);
                    let _ = ready.send(());
                    for message in signals {
                        let Ok((namespace, key, changed)) = message.body().deserialize::<(String, String, zbus::zvariant::OwnedValue)>() else {
                            continue;
                        };
                        if namespace == NAMESPACE && key == KEY {
                            publish(portal_code(changed));
                        }
                    }
                    // A lost portal feed must not leave a stale preference indefinitely.
                    publish(None);
                    Ok(())
                };
                if let Err(error) = run() {
                    log::debug!("appearance portal unavailable: {error}");
                }
            })
            .ok()?;
        let _ = wait.recv_timeout(std::time::Duration::from_millis(250));
        Some(Box::new(move || decode(u32::from(value.load(Ordering::Relaxed)))))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = ctx;
        None
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    #[test]
    fn portal_variants_are_numeric_and_bounded() {
        use zbus::zvariant::{OwnedValue, Value};
        assert_eq!(super::portal_code(OwnedValue::from(1_u32)), Some(1));
        let nested = Value::Value(Box::new(Value::Value(Box::new(Value::U32(2)))));
        let owned = OwnedValue::try_from(nested).ok();
        assert_eq!(owned.and_then(super::portal_code), Some(2));
        let mut value = Value::U32(1);
        for _ in 0..5 {
            value = Value::Value(Box::new(value));
        }
        assert_eq!(OwnedValue::try_from(value).ok().and_then(super::portal_code), None);
        assert_eq!(OwnedValue::try_from(Value::from("prefer-dark")).ok().and_then(super::portal_code), None);
    }

    #[test]
    fn standard_portal_color_scheme() {
        assert_eq!(super::decode(0), None);
        assert_eq!(super::decode(1), Some(egui::Theme::Dark));
        assert_eq!(super::decode(2), Some(egui::Theme::Light));
        assert_eq!(super::decode(3), None);
        assert_eq!(super::decode(u32::MAX), None);
    }
}
