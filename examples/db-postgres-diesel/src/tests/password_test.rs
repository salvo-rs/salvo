#[cfg(test)]
pub mod test{
    use crate::utils::utils::{hash_password, verify_password};

    #[test]
    fn test_hash_password_success() {
        let password = "MySecurePassword123!";
        let hash = hash_password(password).unwrap();
        assert_ne!(hash, password);
        assert!(hash.starts_with("$argon2"));
    }

    #[test]
    fn test_verify_password_success() {
        let password = "MySecret";
        let hash = hash_password(password).unwrap();
        assert!(verify_password(password, &hash));
    }

    #[test]
    fn test_verify_password_failure() {
        let password = "MySecret";
        let hash = hash_password(password).unwrap();
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn test_password_hash_and_verify() {
        let password = "MySecret123";
        let hash = hash_password(password).expect("Failed to hash password");
        assert!(verify_password(password, &hash));
        assert!(!verify_password("wrong_password", &hash));
    }


}
