pub fn valid_client_id(id: &str) -> bool {
    let trimmed = id.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 32
        && trimmed.bytes().all(|byte| byte.is_ascii_digit())
}

pub fn has_credentials(client_id: &str) -> bool {
    valid_client_id(client_id)
}
