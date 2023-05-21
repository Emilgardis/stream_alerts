#[cfg(feature = "ssr")]
pub const COOKIE_AUTH_USER_LOGIN: &str = "user_login";

#[cfg(feature = "ssr")]
pub type AuthContext = axum_login::extractors::AuthContext<i64, User, axum_login::memory_store::MemoryStore<i64, User>>;


#[derive(Clone, Debug)]
pub struct User {
    pub name: String,
    pub id: i64,
    pub password_hash: Vec<u8>,
}

#[cfg(feature = "ssr")]
impl axum_login::AuthUser<i64, ()> for User {
    fn get_id(&self) -> i64 {
        self.id
    }

    fn get_password_hash(&self) -> axum_login::secrecy::SecretVec<u8> {
        axum_login::secrecy::SecretVec::new(self.password_hash.clone())
    }
}