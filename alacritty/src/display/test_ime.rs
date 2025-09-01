#[cfg(test)]
mod tests {
    use crate::display::Ime;
    use std::time::Duration;
    
    #[test]
    fn test_ime_space_commit_suppression() {
        let mut ime = Ime::default();
        
        // Initially should not suppress space
        assert!(!ime.should_suppress_key(" "));
        
        // Mark a space commit
        ime.mark_commit("안녕 ");
        
        // Should now suppress the next space key
        assert!(ime.should_suppress_key(" "));
        
        // After calling should_suppress_key once, it should reset
        assert!(!ime.should_suppress_key(" "));
    }
    
    #[test]
    fn test_ime_space_commit_timeout() {
        let mut ime = Ime::default();
        
        // Mark a space commit
        ime.mark_commit("테스트 ");
        
        // Manually set the timestamp to be older than 10ms to simulate timeout
        if let Some((_, text)) = &ime.last_commit {
            ime.last_commit = Some((std::time::Instant::now() - Duration::from_millis(15), text.clone()));
        }
        
        // Should not suppress space key after timeout
        assert!(!ime.should_suppress_key(" "));
    }
}