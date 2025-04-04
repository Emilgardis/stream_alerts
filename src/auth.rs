#[cfg(feature = "ssr")]
use std::sync::Arc;
#[cfg(feature = "ssr")]
use tokio::sync::RwLock;
#[cfg(feature = "ssr")]
use crate::opts::Opts;

#[cfg(feature = "ssr")]
pub const COOKIE_AUTH_USER_LOGIN: &str = "user_login";

#[cfg(feature = "ssr")]
pub type AuthContext = axum_login::AuthManager<User, SessionStore>;
#[cfg(feature = "ssr")]
pub type AuthSession = axum_login::AuthSession<Users>;

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
impl axum_login::AuthnBackend for Users {
    type User = User;
    type Credentials = (String, Vec<u8>);
    type Error = std::convert::Infallible;

    async fn get_user(
        &self,
        user_id: &axum_login::UserId<Self>,
    ) -> Result<Option<Self::User>, Self::Error> {
        Ok(self.users.read().await.get(user_id).cloned())
    }

    async fn authenticate(
        &self,
        (user, password): Self::Credentials,
    ) -> Result<Option<Self::User>, Self::Error> {
        Ok(self.get(&user, password.as_slice()).await)
    }
}

#[cfg(feature = "ssr")]
impl axum_login::AuthUser for User {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.password_hash.as_bytes()
    }
}

#[cfg(feature = "ssr")]
pub type SessionStore = axum_login::tower_sessions::MemoryStore;

#[cfg(feature = "ssr")]
pub async fn setup(
    opts: &Opts,
) -> Result<
    axum_login::AuthManagerLayer<Users, SessionStore>,
    eyre::Report,
> {


    let user_store = Users::empty();

    user_store.write().await.insert(
        0,
        User::new(
            "admin".to_owned(),
            0,
            opts.admin_password.secret().as_bytes(),
        )?,
    );

    // Session layer.
    let session_store = SessionStore::default();
    let session_layer = axum_login::tower_sessions::SessionManagerLayer::new(session_store)
        .with_secure(true)
        .with_name("stream_alerts_session");

    // Auth service.
    let auth_layer = axum_login::AuthManagerLayerBuilder::new(user_store, session_layer).build();
    Ok(auth_layer)
}

//
