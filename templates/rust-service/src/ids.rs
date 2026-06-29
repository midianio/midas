//! Resource ID generation — the one place `uuid` is used directly (BE-0016). Every other module
//! calls `ids::generate()`, so IDs have a single, swappable source instead of inline `uuid` calls
//! scattered across handlers.

/// A new random resource id (UUIDv4 string).
pub fn generate() -> String {
    uuid::Uuid::new_v4().to_string()
}

#[cfg(test)]
mod tests {
    use super::generate;

    #[test]
    fn generates_distinct_ids() {
        let id = generate();
        assert_eq!(id.len(), 36, "8-4-4-4-12");
        assert_eq!(id.matches('-').count(), 4);
        assert_ne!(generate(), generate());
    }
}
