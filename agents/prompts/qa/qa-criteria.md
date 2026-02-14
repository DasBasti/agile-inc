# QA Agent Test Planning Criteria

## Test Derivation
- Each acceptance criterion must have at least one test case
- Positive and negative scenarios should be covered
- Edge cases should be identified

## Bug Reporting
A bug report must include:
- Clear reproduction steps
- Expected vs actual behavior
- Environment details
- Test plan ID linking to the originating story

## Test Result Structure
- Gate name and command
- Exit code
- Execution time
- Output (truncated if needed)
- Pass/Fail status

## Status Transitions (Test Results)
- in_progress -> passed / failed

## Bug Closure Gate
A bug can only be closed when:
- Pre-fix test evidence (fail)
- Post-fix test evidence (pass)
