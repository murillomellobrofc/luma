//! Som de notificação via libcanberra (carregada em runtime).
//!
//! H54.P6. Contexto criado uma vez no startup, reusado pra cada play.
//! Falha em silêncio: se a libcanberra.so não estiver instalada, se o
//! contexto falhar de criar, se o arquivo não existir, ou se o play
//! retornar erro — nada toca, sem crash.
//!
//! Por que dlopen em vez de link direto: o build não precisa de
//! `libcanberra-dev`. A `.so.0` runtime já está em qualquer distro com
//! libcanberra0 instalada. Falha em silêncio funciona em build/dev.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use libloading::{Library, Symbol};

// ─── FFI types (opaque) ───────────────────────────────────────────────
#[repr(C)]
struct CaContext {
    _private: [u8; 0],
}
#[repr(C)]
struct CaProplist {
    _private: [u8; 0],
}

// Property keys conforme `<canberra.h>`. Strings null-terminadas.
const CA_PROP_MEDIA_FILENAME: &CStr =
    match CStr::from_bytes_with_nul(b"media.filename\0") {
        Ok(s) => s,
        Err(_) => panic!("const CStr"),
    };
const CA_PROP_MEDIA_ROLE: &CStr =
    match CStr::from_bytes_with_nul(b"media.role\0") {
        Ok(s) => s,
        Err(_) => panic!("const CStr"),
    };

// Signatures necessárias.
type CaContextCreate = unsafe extern "C" fn(*mut *mut CaContext) -> c_int;
type CaContextDestroy = unsafe extern "C" fn(*mut CaContext) -> c_int;
type CaContextPlayFull = unsafe extern "C" fn(
    *mut CaContext,
    u32,
    *mut CaProplist,
    *mut c_void, // callback (NULL)
    *mut c_void, // userdata (NULL)
) -> c_int;
type CaProplistCreate = unsafe extern "C" fn(*mut *mut CaProplist) -> c_int;
type CaProplistDestroy = unsafe extern "C" fn(*mut CaProplist) -> c_int;
type CaProplistSets = unsafe extern "C" fn(
    *mut CaProplist,
    *const c_char,
    *const c_char,
) -> c_int;

/// Engine de som. Carrega libcanberra.so em runtime e mantém um
/// contexto + ponteiros pras 6 funções que usamos. Compartilhado
/// entre threads via Send manual (raw pointer).
pub struct SoundEngine {
    // Library tem que viver enquanto o ctx existir.
    _lib: Library,
    ctx: *mut CaContext,
    create_proplist: CaProplistCreate,
    destroy_proplist: CaProplistDestroy,
    set_proplist: CaProplistSets,
    play_full: CaContextPlayFull,
    destroy_ctx: CaContextDestroy,
    /// Último Instant em que tocou algo. Usado pro debounce.
    last_played: Mutex<Option<Instant>>,
    debounce: Duration,
    /// Serializa o uso do `ctx` entre threads. libcanberra NÃO garante
    /// reentrância no mesmo ca_context, então play/destroy concorrentes
    /// seriam data race. Segurado durante toda a sequência de play.
    ctx_lock: Mutex<()>,
}

// SAFETY: o único ponteiro não-Sync é `ctx`, e todo acesso a ele
// (play_full, destroy_ctx) é serializado por `ctx_lock`. last_played é
// Mutex; os fn pointers são Copy/Send; Library é Send + Sync.
unsafe impl Send for SoundEngine {}
unsafe impl Sync for SoundEngine {}

impl SoundEngine {
    /// Tenta carregar libcanberra e criar o contexto. Retorna None se
    /// qualquer passo falhar (`.so` ausente, link errado, contexto
    /// não criado). Caller trata None como "som desligado".
    pub fn try_new(debounce_ms: u64) -> Option<Self> {
        // Tenta libcanberra.so.0 primeiro (versão runtime estável).
        let lib = unsafe {
            Library::new("libcanberra.so.0")
                .or_else(|_| Library::new("libcanberra.so"))
                .ok()?
        };
        // Resolve cada símbolo. Se algum faltar, aborta.
        let create_ctx: Symbol<CaContextCreate> =
            unsafe { lib.get(b"ca_context_create").ok()? };
        let destroy_ctx: Symbol<CaContextDestroy> =
            unsafe { lib.get(b"ca_context_destroy").ok()? };
        let play_full: Symbol<CaContextPlayFull> =
            unsafe { lib.get(b"ca_context_play_full").ok()? };
        let create_proplist: Symbol<CaProplistCreate> =
            unsafe { lib.get(b"ca_proplist_create").ok()? };
        let destroy_proplist: Symbol<CaProplistDestroy> =
            unsafe { lib.get(b"ca_proplist_destroy").ok()? };
        let set_proplist: Symbol<CaProplistSets> =
            unsafe { lib.get(b"ca_proplist_sets").ok()? };
        // Captura ponteiros das funções (raw fn pointer pra não amarrar
        // o lifetime do Symbol — Library vive enquanto SoundEngine).
        let create_ctx_fn = *create_ctx;
        let destroy_ctx_fn = *destroy_ctx;
        let play_full_fn = *play_full;
        let create_proplist_fn = *create_proplist;
        let destroy_proplist_fn = *destroy_proplist;
        let set_proplist_fn = *set_proplist;
        drop(create_ctx);
        drop(destroy_ctx);
        drop(play_full);
        drop(create_proplist);
        drop(destroy_proplist);
        drop(set_proplist);
        // Cria contexto.
        let mut ctx: *mut CaContext = std::ptr::null_mut();
        let rc = unsafe { create_ctx_fn(&mut ctx) };
        if rc != 0 || ctx.is_null() {
            tracing::warn!("sound: ca_context_create falhou rc={rc}");
            return None;
        }
        tracing::info!("sound: libcanberra carregada, contexto criado");
        Some(Self {
            _lib: lib,
            ctx,
            create_proplist: create_proplist_fn,
            destroy_proplist: destroy_proplist_fn,
            set_proplist: set_proplist_fn,
            play_full: play_full_fn,
            destroy_ctx: destroy_ctx_fn,
            last_played: Mutex::new(None),
            debounce: Duration::from_millis(debounce_ms),
            ctx_lock: Mutex::new(()),
        })
    }

    /// Toca um arquivo OGG/WAV via `ca_context_play_full` + proplist.
    /// Falha em silêncio se path inválido, arquivo ausente, ou play
    /// retornar erro. Aplica debounce — se o último play foi há menos
    /// que `debounce`, ignora.
    pub fn play_file(&self, path: &Path) {
        // Debounce.
        let now = Instant::now();
        if let Ok(mut g) = self.last_played.lock() {
            if let Some(last) = *g {
                if now.duration_since(last) < self.debounce {
                    return;
                }
            }
            *g = Some(now);
        }
        // Path como CString.
        let path_str = match path.to_str() {
            Some(s) => s,
            None => return,
        };
        let path_c = match CString::new(path_str) {
            Ok(c) => c,
            Err(_) => return,
        };
        let role_c = match CString::new("event") {
            Ok(c) => c,
            Err(_) => return,
        };
        // Serializa o uso do ctx (e do proplist) por toda a sequência.
        let _ctx_guard = match self.ctx_lock.lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        // Cria proplist.
        let mut pl: *mut CaProplist = std::ptr::null_mut();
        let rc = unsafe { (self.create_proplist)(&mut pl) };
        if rc != 0 || pl.is_null() {
            return;
        }
        // Seta media.filename + media.role=event.
        unsafe {
            (self.set_proplist)(pl, CA_PROP_MEDIA_FILENAME.as_ptr(), path_c.as_ptr());
            (self.set_proplist)(pl, CA_PROP_MEDIA_ROLE.as_ptr(), role_c.as_ptr());
        }
        // Play (id=0, sem callback).
        let rc = unsafe {
            (self.play_full)(
                self.ctx,
                0,
                pl,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if rc != 0 {
            tracing::debug!(
                "sound: ca_context_play_full rc={rc} file={path_str}"
            );
        }
        unsafe {
            (self.destroy_proplist)(pl);
        }
    }
}

impl Drop for SoundEngine {
    fn drop(&mut self) {
        // Espera qualquer play em andamento antes de destruir o ctx.
        let _ctx_guard = self.ctx_lock.lock();
        if !self.ctx.is_null() {
            unsafe {
                (self.destroy_ctx)(self.ctx);
            }
            self.ctx = std::ptr::null_mut();
        }
    }
}
