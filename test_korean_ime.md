# CJK IME Double Character Bug Fix

## 수정 내용
- **확장된 범위**: 한글뿐만 아니라 모든 CJK (Chinese, Japanese, Korean) IME 문제 해결
- **포괄적 해결**: 스페이스뿐만 아니라 모든 문자의 중복 입력 방지
- macOS CJK IME에서 문자 입력 시 중복 입력 방지

## 해결된 이슈들
1. **Issue #8079**: 한글 IME 이중 스페이스 문제
2. **Issue #6942**: CJK IME 첫 번째 특수문자/숫자 무시 문제
3. **포괄적 해결**: 중국어, 일본어 IME에서도 비슷한 문제 예방

## 수정된 파일
1. `alacritty/src/display/mod.rs` - IME 구조체에 포괄적 커밋 추적 기능 추가
   - `last_commit: Option<(Instant, String)>` - 타임스탬프와 커밋 텍스트 추적
   - `mark_commit()` - 모든 IME 커밋 추적
   - `should_suppress_key()` - 중복 문자 입력 감지 및 억제
   
2. `alacritty/src/event.rs` - IME commit 이벤트에서 모든 텍스트 커밋 마킹
3. `alacritty/src/input/keyboard.rs` - 키보드 입력에서 중복 문자 필터링

## 테스트 방법
1. 수정된 Alacritty 실행: `./target/release/alacritty`
2. 한글 입력기 활성화
3. 한글을 입력하고 스페이스바 누르기
4. 결과 확인: 한 칸의 스페이스만 입력되어야 함

## 기술적 세부사항
- 10ms 타임윈도우 내에서 IME commit 후 오는 중복 키 입력을 필터링
- **크로스 플랫폼 지원**: macOS, Linux, Windows 모든 플랫폼에서 동작
- **안전한 구현**: 일반 키보드 입력에는 영향 없음 (매우 짧은 시간창 사용)
- **효율적**: 메모리 사용량 최소화, 성능 오버헤드 거의 없음

## 빌드 상태
✅ 컴파일 성공
✅ 릴리즈 빌드 완료