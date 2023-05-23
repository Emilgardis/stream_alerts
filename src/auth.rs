#[cfg(feature = "ssr")]
use std::sync::Arc;
#[cfg(feature = "ssr")]
use tokio::sync::RwLock;

use crate::opts::Opts;

#[cfg(feature = "ssr")]
pub const COOKIE_AUTH_USER_LOGIN: &str = "user_login";

#[cfg(feature = "ssr")]
pub type AuthContext = axum_login::extractors::AuthContext<i64, User, Users, Role>;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct User {
    pub name: String,
    pub id: i64,
    /// Password in [scrypt::password_hash::PasswordHashString] format
    password_hash: String,
}

#[cfg(feature = "ssr")]
pub static RNG: once_cell::sync::Lazy<std::sync::Mutex<rand::rngs::StdRng>> =
    once_cell::sync::Lazy::new(|| {
        let seed = if cfg!(debug_assertions) {
            tracing::warn!("using insecure rng");
            std::array::from_fn(|i| (10 + i) as u8)
        } else {
            rand::Rng::gen(&mut rand::thread_rng())
        };
        std::sync::Mutex::new(<rand::rngs::StdRng as rand::SeedableRng>::from_seed(seed))
    });
impl User {
    #[cfg(feature = "ssr")]
    pub fn new(
        name: String,
        id: i64,
        password: &[u8],
    ) -> Result<Self, scrypt::password_hash::errors::Error> {
        use scrypt::password_hash::PasswordHasher;

        let salt = scrypt::password_hash::SaltString::generate(&mut *RNG.lock().unwrap());
        let password_hash = scrypt::Scrypt.hash_password(password, &salt)?;
        Ok(Self {
            name,
            id,
            password_hash: password_hash.serialize().to_string(),
        })
    }

    #[cfg(feature = "ssr")]
    pub fn password(&self) -> scrypt::password_hash::PasswordHash {
        scrypt::password_hash::PasswordHash::new(&self.password_hash).unwrap()
    }
}

#[derive(Clone)]
#[cfg(feature = "ssr")]
pub struct Users {
    pub users: Arc<RwLock<std::collections::HashMap<i64, User>>>,
}

#[cfg(feature = "ssr")]
impl std::fmt::Debug for Users {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("Users");
        if let Ok(m) = &self.users.try_read() {
            debug.field("users", &m.len());
        } else {
            debug.field("users", &"locked");
        }
        debug.finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {}

#[cfg(feature = "ssr")]
impl Users {
    pub fn empty() -> Self {
        Self {
            users: Default::default(),
        }
    }

    async fn write(&self) -> tokio::sync::RwLockWriteGuard<std::collections::HashMap<i64, User>> {
        self.users.write().await
    }

    pub async fn get(&self, username: &str, password: &[u8]) -> Option<User> {
        let users = self.users.read().await;
        let user = users.values().find(|user| user.name == username)?;

        if user
            .password()
            .verify_password(&[&scrypt::Scrypt], password)
            .is_ok()
        {
            Some(user.clone())
        } else {
            None
        }
    }
}

#[cfg(feature = "ssr")]
#[async_trait::async_trait]
impl axum_login::UserStore<i64, Role> for Users {
    type User = User;

    async fn load_user(&self, user_id: &i64) -> Result<Option<Self::User>, eyre::Report> {
        Ok(self.users.read().await.get(user_id).cloned())
    }
}

#[cfg(feature = "ssr")]
impl axum_login::AuthUser<i64, Role> for User {
    fn get_id(&self) -> i64 { self.id }

    fn get_password_hash(&self) -> axum_login::secrecy::SecretVec<u8> {
        tracing::info!("getting password hash");
        axum_login::secrecy::SecretVec::new(self.password().hash.unwrap().as_bytes().to_vec())
    }
}

#[cfg(not(debug_assertions))]
#[cfg(feature = "ssr")]
pub type SessionStore = axum_login::axum_sessions::async_session::MemoryStore;
#[cfg(debug_assertions)]
#[cfg(feature = "ssr")]
pub type SessionStore = axum_login::axum_sessions::async_session::CookieStore;

#[cfg(feature = "ssr")]
pub async fn setup(
    opts: &Opts,
) -> Result<
    (
        axum_login::axum_sessions::SessionLayer<SessionStore>,
        axum_login::AuthLayer<Users, i64, User, Role>,
        Users,
    ),
    eyre::Report,
> {
    use rand::Rng;

    let user_store = Users::empty();

    user_store.write().await.insert(
        0,
        User::new(
            "admin".to_owned(),
            0,
            opts.admin_password.secret().as_bytes(),
        )?,
    );

    let secret = RNG.lock().unwrap().gen::<[u8; 64]>();

    let session_store = SessionStore::new();
    let session_layer = axum_login::axum_sessions::SessionLayer::new(session_store, &secret)
        .with_cookie_name("stream_alerts_session")
        .with_secure(true);
    let auth_layer = axum_login::AuthLayer::new(user_store.clone(), &secret);
    Ok((session_layer, auth_layer, user_store))
}

//
