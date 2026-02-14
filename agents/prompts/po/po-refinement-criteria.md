# PO Story Refinement Criteria

A story is considered READY for development when ALL of the following criteria are met:

## 1. Acceptance Criteria (AC)
- [ ] Clear and unambiguous description
- [ ] Each AC is testable/verifiable
- [ ] AC covers positive and negative scenarios
- [ ] AC is independent of implementation details

## 2. Definition of Done (DoD)
- [ ] At least one machine-checkable gate defined
- [ ] DoD gates are deterministic (pass/fail)
- [ ] DoD gates are fast to execute (< 5 min recommended)
- [ ] Examples: cargo test, cargo fmt --check, cargo clippy

## 3. Story Structure
- [ ] Title is concise (< 50 chars)
- [ ] Body provides sufficient context
- [ ] No conflicting requirements in AC
- [ ] Dependencies on other stories are documented

## 4. Estimability
- [ ] Scope is clear enough to estimate
- [ ] No technical unknowns blocking implementation
- [ ] Security/compliance considerations addressed if needed

## Status Transitions
- draft -> refining -> ready -> in_progress -> done
- Stories in "draft" should not be published to Dev
