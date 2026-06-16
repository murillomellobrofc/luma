use std::path::PathBuf;

use tracing::info;

/// Atalho da coluna direita do menu (Documentos / Downloads / etc).
/// Campos preenchidos por `load()`; serão lidos quando o painel
/// Places for renderizado/clicável.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct Place {
    pub label: String,
    /// Caminho/URI passado pro xdg-open. Pode ser absoluto ("/"),
    /// home-relative ("$HOME/Documents") ou URI ("file:///etc/...").
    pub target: String,
}

/// Lista padrão de lugares. Pastas que não existem no `$HOME` são
/// puladas pra não ter botão que abre erro. "Sistema" aponta pro / e
/// sempre aparece.
pub fn load() -> Vec<Place> {
    let mut places = Vec::with_capacity(6);
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home".to_string());
    let home_path = PathBuf::from(&home);

    for (label, subdir) in [
        ("Documentos", "Documents"),
        ("Downloads", "Downloads"),
        ("Imagens", "Pictures"),
        ("Música", "Music"),
        ("Vídeos", "Videos"),
    ] {
        let candidate = home_path.join(subdir);
        if candidate.exists() {
            places.push(Place {
                label: label.to_string(),
                target: candidate.to_string_lossy().into_owned(),
            });
        }
    }

    places.push(Place {
        label: "Sistema".to_string(),
        target: "/".to_string(),
    });

    info!("{} lugares carregados pro menu", places.len());
    places
}

