#[cfg(test)]
mod ime_integration_tests {
    use crate::display::Ime;
    use std::time::{Duration, Instant};
    
    #[test]
    fn test_ime_space_commit_suppression() {
        let mut ime = Ime::default();
        
        // 초기 상태에서는 스페이스 억제하지 않음
        assert!(!ime.should_suppress_key(" "));
        
        // 스페이스로 끝나는 텍스트 커밋
        ime.mark_commit("안녕 ");
        
        // 바로 이어지는 스페이스 키는 억제되어야 함
        assert!(ime.should_suppress_key(" "));
        
        // 다시 체크하면 이미 억제되었으므로 false
        assert!(!ime.should_suppress_key(" "));
    }
    
    #[test]
    fn test_ime_non_space_commit_no_suppression() {
        let mut ime = Ime::default();
        
        // 스페이스로 끝나지 않는 텍스트 커밋
        ime.mark_commit("안녕");
        
        // 스페이스 키는 억제되지 않아야 함
        assert!(!ime.should_suppress_key(" "));
        
        // 다른 문자도 억제되지 않음
        assert!(!ime.should_suppress_key("a"));
    }
    
    #[test]
    fn test_ime_commit_timeout() {
        let mut ime = Ime::default();
        
        // 스페이스로 끝나는 텍스트 커밋
        ime.mark_commit("테스트 ");
        
        // 타임스탬프를 15ms 전으로 조작 (10ms 윈도우 초과)
        if let Some((_, text)) = &ime.last_commit {
            ime.last_commit = Some((Instant::now() - Duration::from_millis(15), text.clone()));
        }
        
        // 타임아웃되어 억제하지 않아야 함
        assert!(!ime.should_suppress_key(" "));
    }
    
    #[test]
    fn test_ime_multiple_character_suppression() {
        let mut ime = Ime::default();
        
        // 특수문자로 끝나는 커밋
        ime.mark_commit("테스트:");
        
        // 해당 특수문자는 억제
        assert!(ime.should_suppress_key(":"));
        
        // 다른 문자는 억제 안됨
        assert!(!ime.should_suppress_key(";"));
    }
    
    #[test]
    fn test_ime_korean_composition_patterns() {
        let mut ime = Ime::default();
        
        // 일반적인 한글 패턴들 테스트
        let test_cases = vec![
            ("안녕 ", " "), // 스페이스로 끝나는 경우
            ("하세요.", "."), // 마침표로 끝나는 경우  
            ("무엇인가?", "?"), // 물음표로 끝나는 경우
            ("좋습니다!", "!"), // 느낌표로 끝나는 경우
        ];
        
        for (commit_text, expected_suppressed_key) in test_cases {
            ime.mark_commit(commit_text);
            
            // 예상되는 키는 억제되어야 함
            assert!(ime.should_suppress_key(expected_suppressed_key), 
                    "Failed to suppress '{}' after commit '{}'", 
                    expected_suppressed_key, commit_text);
                    
            // 다른 키는 억제되지 않아야 함
            assert!(!ime.should_suppress_key("x"), 
                    "Unexpectedly suppressed 'x' after commit '{}'", commit_text);
        }
    }
    
    #[test] 
    fn test_ime_empty_commit() {
        let mut ime = Ime::default();
        
        // 빈 텍스트 커밋
        ime.mark_commit("");
        
        // 어떤 키도 억제하지 않아야 함
        assert!(!ime.should_suppress_key(" "));
        assert!(!ime.should_suppress_key("a"));
    }
    
    #[test]
    fn test_ime_rapid_succession() {
        let mut ime = Ime::default();
        
        // 연속된 커밋
        ime.mark_commit("첫번째 ");
        assert!(ime.should_suppress_key(" "));
        
        // 새로운 커밋
        ime.mark_commit("두번째!");
        assert!(ime.should_suppress_key("!"));
        assert!(!ime.should_suppress_key(" ")); // 이전 커밋과는 무관
    }
}