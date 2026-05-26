//! Integração com power-profiles-daemon via D-Bus (system bus).
//! H57: applet de bateria. Lê o profile ativo + lista de profiles
//! disponíveis, e troca via property write. Falha em silêncio se o
//! daemon não estiver rodando (a seção de modo de energia some no
//! popup nesse caso).

/// Snapshot do estado do power-profiles-daemon. None quando o
/// daemon não está disponível.
#[derive(Debug, Clone)]
pub struct PowerProfilesState {
    /// Profile ativo: "power-saver", "balanced", "performance" (ou
    /// subset do que o hardware oferece).
    pub active: String,
    /// Profiles disponíveis na ordem canônica
    /// (Economia → Equilibrado → Desempenho).
    pub available: Vec<String>,
}

impl PowerProfilesState {
    /// Index do profile ativo dentro de `available`. None se não
    /// estiver na lista (estado inconsistente, defensivo).
    pub fn active_idx(&self) -> Option<usize> {
        self.available.iter().position(|p| p == &self.active)
    }
}

/// Label PT-BR pra um profile name. H57.
pub fn profile_label(name: &str) -> &'static str {
    match name {
        "power-saver" => "Economia",
        "balanced" => "Equilibrado",
        "performance" => "Desempenho",
        _ => "—",
    }
}

/// Ordena uma lista de profiles na ordem canônica (Economia primeiro,
/// Desempenho por último). Profiles desconhecidos vão pro fim.
fn canonical_order(name: &str) -> u8 {
    match name {
        "power-saver" => 0,
        "balanced" => 1,
        "performance" => 2,
        _ => 99,
    }
}

/// Lê o estado atual via D-Bus system bus. Retorna None se o daemon
/// não responde (não instalado, não rodando, sem permissão).
pub fn read_state() -> Option<PowerProfilesState> {
    let conn = zbus::blocking::Connection::system().ok()?;
    let proxy = zbus::blocking::Proxy::new(
        &conn,
        "net.hadess.PowerProfiles",
        "/net/hadess/PowerProfiles",
        "net.hadess.PowerProfiles",
    )
    .ok()?;
    // ActiveProfile (s).
    let active: String = proxy.get_property("ActiveProfile").ok()?;
    // Profiles (aa{sv}). Cada entry tem chave "Profile" com o nome.
    let profiles_var: Vec<std::collections::HashMap<String, zbus::zvariant::OwnedValue>> =
        proxy.get_property("Profiles").ok()?;
    let mut available: Vec<String> = profiles_var
        .into_iter()
        .filter_map(|m| {
            m.get("Profile")
                .and_then(|v| String::try_from(v.clone()).ok())
        })
        .collect();
    available.sort_by_key(|n| canonical_order(n));
    if available.is_empty() {
        return None;
    }
    Some(PowerProfilesState { active, available })
}

/// Seta o profile ativo via property write. Falha em silêncio
/// (retorna false) se daemon ausente ou nome inválido. H57.
pub fn set_active(name: &str) -> bool {
    let Ok(conn) = zbus::blocking::Connection::system() else {
        return false;
    };
    let Ok(proxy) = zbus::blocking::Proxy::new(
        &conn,
        "net.hadess.PowerProfiles",
        "/net/hadess/PowerProfiles",
        "net.hadess.PowerProfiles",
    ) else {
        return false;
    };
    match proxy.set_property("ActiveProfile", name) {
        Ok(()) => {
            tracing::info!("power profile -> {name}");
            true
        }
        Err(e) => {
            tracing::warn!("power profile set falhou: {e}");
            false
        }
    }
}
