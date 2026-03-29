# Contributor Notes

This document keeps contributor-facing sequencing notes that are useful when extending
`latexd` beyond the current milestone baseline.

## First Eight PRs

### PR 1

`hmr-protocol`만 만든다.
메시지 타입, serde, TS 타입 생성, reducer 테스트.

### PR 2

`web/packages/viewer-core`와 `web/apps/viewer`에 preview bootstrap을 붙인다.
가짜 서버 메시지로 페이지 교체 없이 full PDF reload만 지원.

### PR 3

`latexd`를 만들고 파일 감시 + compile spawn + WS push를 붙인다.
여기까지는 외부 컴파일러만 사용.

### PR 4

`tex-world`와 `00README` parser를 만든다.
root compile semantics 테스트 추가.

### PR 5

dependency trace DB를 만든다.
no-op short-circuit까지 구현.

### PR 6

`tex-lexer`와 token interner를 만든다.
property/fuzz 포함.

### PR 7

`tex-vm`에 expansion/group/register 최소셋을 넣는다.
transcript golden tests 작성.

### PR 8

mini format fixture를 로드하고 micro docs를 internal engine으로 돌린다.

이 8개가 끝나면 **프론트/백 HMR UX는 이미 존재**하고, 그 아래 코어만 Rust 엔진으로
바꿔 나가면 된다.

## Pitfalls To Avoid

1. **처음부터 Ghostscript FFI에 들어가는 것**
   먼저 PDF.js bootstrap과 mock renderer로 viewer contract를 고정해야 한다.

2. **AST 중심 사고**
   TeX는 AST보다 expansion trace와 VM state가 핵심이다.

3. **token-level incremental**
   TeX에서는 비용이 이득을 먹어치울 확률이 높다.

4. **page number 기반 캐시**
   page id 기반으로 가야 한다.

5. **package별 예외처리 남발**
   primitive coverage와 trace infrastructure를 올려야 한다.

6. **arXiv corpus 없이 개발하는 것**
   실제 source+PDF pair가 있어야 어디까지 왔는지 알 수 있다.

## Summary

한 줄로 요약하면 이렇게 가면 된다.

1. **외부 컴파일러로 세로 슬라이스를 먼저 만든다.**
2. **`00README`/arXiv profile/root compile semantics를 먼저 고정한다.**
3. **Salsa류 query는 바깥쪽만 쓰고, TeX VM은 checkpoint/page 단위로 재실행한다.**
4. **preamble snapshot과 shipout checkpoint를 가장 먼저 구현한다.**
5. **viewer는 PDF.js로 시작하고, page viewer → tile viewer로 진화시킨다.**
6. **Ghostscript는 최종 프리뷰 가속기이지, incremental build의 본체가 아니다.**
7. **테스트는 micro → compat → arXiv paired corpus → mutation/perf 순으로 키운다.**
