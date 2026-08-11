# Milestone Breakdown

이 문서는 너무 큰 milestone이던 `M11`, `M12`를 실제 완료 기준에 맞춰 `M11.x`, `M12.x`로 나눈 현재 기준표다.

기준 문서:
- [`README.md`](/home/seorii/dev/hancomac/latexd/README.md)
- [`PROGRESS.md`](/home/seorii/dev/hancomac/latexd/PROGRESS.md)
- [`docs/m12-checklist.md`](/home/seorii/dev/hancomac/latexd/docs/m12-checklist.md)
- [`docs/work-backlog.md`](/home/seorii/dev/hancomac/latexd/docs/work-backlog.md)
- [`docs/renderer-session-plan.md`](/home/seorii/dev/hancomac/latexd/docs/renderer-session-plan.md)
- [`docs/real-rendering-plan.md`](/home/seorii/dev/hancomac/latexd/docs/real-rendering-plan.md)
- [`docs/vm-semantic-foundation-plan.md`](/home/seorii/dev/hancomac/latexd/docs/vm-semantic-foundation-plan.md)
- [`docs/math-rendering-plan.md`](/home/seorii/dev/hancomac/latexd/docs/math-rendering-plan.md)

## Status Snapshot

- 기준 시점: `2026-08-10`
- 기준 commit: `94d277e` (`refactor(vm): isolate control sequence scopes`)
- `P0.3a` positioned Type1 outline sub-slice는 `2c2b2db`에 구현됐고 focused
  verification은 green이다. `8cbea9d`는 build-time raw/gzip/Brotli 크기
  예산과 SHA-256 identity, 별도 fresh-process Node compile 표본을 재현하는
  WASI cost reporter를 추가했다. 이는 formula/table/image/link/multi-page
  visual 및 broader differential gate를 포함하는 umbrella `P0.3` 완료를
  뜻하지 않는다.
- `M11`: `M11.1`~`M11.4` 완료
- `M12`: `M12.1`~`M12.6` 완료
- 현재 집중 범위: `M13` 단일 VM 실행 의미와 실행 기반 event/IR/checkpoint

이후 `현재 구현`은 landed vertical slice만, `완료 조건`은 phase exit만
가리킨다. 뒤 단계의 slice가 일부 구현됐더라도 앞 단계 exit나 뒤 단계 전체
완료를 자동으로 뜻하지 않는다.

## M11 Split

### M11.1 Concrete Semantic Aux Artifacts

상태: `completed`

실제 기준:
- concrete `semantic.aux`가 revision마다 persisted된다.
- labels, citations, bibliography, TOC surface가 artifact로 round-trip 된다.

### M11.2 Semantic Equality And Backdating

상태: `completed`

실제 기준:
- semantic equality가 raw source equality와 분리돼 있다.
- semantic-equal rebuild는 backdating과 bounded rerun-to-fixpoint를 탄다.

### M11.3 Executed-Source Rewrite Surface

상태: `completed`

실제 기준:
- natbib/biblatex, theorem, float/list, reference surface가 executed source에 raw command를 남기지 않는다.

### M11.4 Replay, Checkpoints, Observability

상태: `completed`

실제 기준:
- semantic replay가 checkpoint/page reuse와 연결돼 있다.
- `build-meta.json`, `semantic-index.json`, executed-source snapshot으로 rerun/replay 판단을 관찰할 수 있다.

## M12 Split

### M12.1 Wrapper-Heavy Corpus Baseline

상태: `completed`

실제 기준:
- split preamble + wrapper-heavy `cls/sty/cfg/def` project shape가 regression corpus에 올라가 있다.
- local preamble/package/class/body failure-recovery chain이 corpus로 고정돼 있다.

### M12.2 Bibliography And Toolchain Realism

상태: `completed`

실제 기준:
- bibliography order/style/tool-version drift, semantically-equal `.bbl`, semantic-change `.bbl`, partial `.bbl` loss/recovery가 regression으로 고정돼 있다.
- natbib/biblatex bibliography surface가 larger family corpus에 올라가 있다.

### M12.3 Package Interaction And Semantic Artifact Tightening

상태: `completed`

실제 기준:
- wrapper package/class interaction, option propagation, semantic discovery drift가 regression으로 고정돼 있다.
- `semantic.aux`, `build-meta.json`, `semantic-index.json`이 representative large family에서 structured expectation을 가진다.

### M12.4 Failure, Recovery, Structured Expectations

상태: `completed`

실제 기준:
- success/backdating/change/failure/recovery가 one corpus harness에서 multi-revision으로 검증된다.
- failure path는 `FAIL-JSON-EXPECT.txt`, recovery path는 JSON metadata expectation까지 포함한다.

### M12.5 Renderer Session Hardening

상태: `completed`

실제 기준:
- actor-owned renderer session, attached revision retention, tile-native path, prewarm, warm-bucket retention, metrics/debug surface가 들어가 있다.

### M12.6 Sync And Editor Hardening

상태: `completed`

실제 기준:
- page-local source/output window, stable `item_id`, canonical `source_hash`, normalized geometry, richer `/api/open-source` payload가 들어가 있다.
- viewer는 stale selection clearing와 same-item re-anchoring을 지원한다.

## Direct Work Policy

이후 작업은 별도 PR 단계로 나누지 않는다. 현재 브랜치에서 다음 규칙으로
직접 진행한다.

- 실패/characterization 테스트를 먼저 추가한다.
- 기계적 파일 이동과 의미 변경을 같은 커밋에 섞지 않는다.
- 하나의 assignment class 또는 event family 단위로 구현하고 검증한다.
- focused test와 지정된 broader lane이 green일 때 conventional commit을 만든다.
- 현재 batch가 red인 상태에서 다음 단계로 넘어가지 않는다.
- 사용자 소유의 unrelated/untracked 파일은 staging하거나 커밋하지 않는다.

## M13: Single VM Execution Semantics

현재 최우선 목표는 수식 노드를 늘리는 것이 아니라 다음 관계를 참으로
만드는 것이다.

```text
SemanticDocumentIr에 존재하는 내용
==
VM이 실제로 실행하여 문서에 기여한 내용
```

상세 상태 모델, 직접 작업 순서, 테스트와 완료 조건은
[`docs/vm-semantic-foundation-plan.md`](/home/seorii/dev/hancomac/latexd/docs/vm-semantic-foundation-plan.md)
를 기준으로 한다.

### M13.0 Characterization

상태: `in progress`

현재 구현:
- local register scope, delimited macro, conditional event suppression,
  macro-generated event, runtime catcode, continuation replay
  characterization이 들어가 있다.
- unsafe continuation reuse는 보수적으로 차단하고 semantic event prefix를
  replay 결과의 authoritative prefix로 유지한다.
- structured diagnostic와 full snapshot equivalence 확대는 계속 진행한다.

- local register assignment
- delimited macro argument
- false conditional 안의 semantic event 억제
- macro-generated math/event
- runtime catcode
- snapshot/replay equivalence

현재 동작을 무조건 golden으로 승인하지 않고, 알려진 semantic divergence는
expected failure로 고정한 뒤 다음 batch에서 제거한다.
완전 continuation이 아닌 checkpoint는 필요한 상태가 명시적으로 비어
있음이 검증되지 않으면 재사용하지 않도록 보수 gate를 추가한다.

### M13.1 Mechanical VM Split

상태: `in progress`

현재 구현:
- command, input, Eqtb, SaveStack, snapshot, semantic family가 독립 module로
  이동했다.
- `ControlSequenceScopes`가 root/current definition, visible lookup, group
  depth/push/pop과 snapshot layer 변환을 소유한다. 기존 layered-map 의미와
  serialized `scopes` schema는 유지한다.
- `lib.rs`는 기준 working tree에서 약 52,200줄이며 source recovery,
  compatibility surface와 주요 execution/state path가 크게 남아 있다.
  따라서 module 파일의 존재를 mechanical facade exit로 보지 않는다.

- `tex-vm/src/lib.rs`를 facade로 축소한다.
- engine/input/mouth/expansion/macro/condition/eqtb/save-stack/assignment/
  snapshot/diagnostic/semantic-sink/source-recovery/compat 모듈로 이동한다.
- public API, serialized schema, event ordering, golden은 바꾸지 않는다.
- scanner는 `source_recovery`로, package/class shim은 `compat`로 격리한다.

완료 조건:
- 기존 behavior와 goldens 동일
- focused VM 및 standard workspace lane green
- 파일 이동 커밋에 semantic diff 없음

### M13.2 Scanner Quarantine And Event Identity

상태: `in progress`

현재 구현:
- 일반 scanner recovery event는 `ScannerRecovery`와 medium confidence를
  명시한다. `RawFallback`은 `Fallback`/fallback, diagnostic은 현재
  `Unknown`/low를 유지하는 characterization으로 고정했다.
- migrated family는 primitive/macro 실행 event로 scanner 결과를
  reconciliation한다.
- serialized `event_id`는 build-local `sequence`로 전환됐다.
- opaque `EventOrigin`과 private-field `EventBuildContext`가 새 write 경로의
  producer/confidence 조합을 검증한다 (`f06bcdf`). scanner helper와 executed
  list item/environment begin-end (`e8840b2`), inline citation/reference/label/link
  (`a9ae789`), loss-aware caption (`e4a5cdb`), heading (`943e580`)이
  `try_from_origin()`으로 이전됐다. active capture snapshot producer 검증은
  `6205f9d`, footnote, math, graphic 이전은 `c94664b`, `7edfd8b`, `75f0803`에
  landed했다. text capture snapshot producer 검증은 `1baa07b`, front matter와
  text 이전은 `5fa5a5c`, `081248e`에 landed했다. table snapshot/table 이전은
  `70090a6`, `a3562f7`, bibliography snapshot/projection-loss origin/typed 이전은
  `9fef0b7`, `9d122b0`, `69e75ea`에 landed했다. raw `with_origin()`과 `new()`는
  wire/fixture contract를 위한 임시 호환 API다.
- production `src/`에서는 `new()`/`with_origin()` 직접 호출을 syntax test로
  차단하고, workspace lib/bin에서는 Clippy `disallowed-methods`로 같은 정책을
  이중 검증한다 (`776d604`). producer/confidence/generated-by를 직접 바꾸는
  production assignment도 syntax test가 차단한다. table raw-fallback 승격과
  text leading-space 재조정은 sequence/source/mode를 보존한 채 typed origin으로
  envelope를 재구성하도록 이전됐다 (`75a79d5`).
- list, environment, heading, caption, graphic, front-matter, bibliography의
  동일한 full-provenance overlap은 `source_locations_overlap()`으로 공통화됐다
  (`decccd7`). matching identity는 primary/related/expansion의 half-open file
  span만 사용하고 `generated_by`, producer/confidence, truncation은 보지 않는다.
  기존 호환 동작대로 모든 related role과 expansion definition span도 포함한다.
  span 규칙이 다른 inline/text/footnote와 producer-coupled insertion anchor는
  이 mechanical batch에서 제외했다.
- lexical false branch뿐 아니라 runtime `\ifnum` false branch의 table
  scanner/fallback event도 executed suppression range로 제거한다. 판정은
  table 시작 anchor에 한정해 cell 내부 phantom/spacing suppression이 visible
  table 전체를 제거하지 않는다.
- phase exit는 열려 있다. 전체 call site 분류와 production migration 뒤
  `new()`는 production 0개이며, 실제 test call은 contract 24개와 incidental
  fixture 73개, 총 97개가 남았다. origin-sensitive semantic-text fixture 3개는
  scanner medium/macro high typed origin으로 이전됐다 (`91a8daa`). guard
  self-test의 source string 안에 있는 `new()` 예시 2개는 이 inventory에서
  제외한다. 남은 fixture 정책, producer-coupled reconciliation anchor와
  sequence/source reuse audit, `ExecutedSourceSlice` interface,
  revision/dependency metadata, shared diagnostic schema와 남은 family leakage
  characterization도 완료되지 않았다. definition span만 공유하는 반복 macro
  invocation의 교차 matching 가능성은 기존 `ARCH-007`의 coarse byte-overlap
  risk에 속하며 execution identity 도입 전에는 의미를 바꾸지 않는다.

2026-08-11 Pro review 결정:
- 새 write 경로는 opaque `EventOrigin`으로 제한하되 기존 wire field/tag와
  permissive legacy deserialization은 유지한다. typed write validation과
  legacy read strictness는 별도 migration이다.
- serialized `Command`를 지금 `CompatCommand`로 rename하지 않는다. `Shim`,
  `BblParser`를 포함한 실제 생산자 분류 뒤 별도 schema migration으로 다룬다.
- lossy executed event의 현재 `Fallback`/low projection과 diagnostic의
  `Unknown`/low projection을 유지한다. taxonomy 정규화는 consumer와
  reconciliation audit 뒤 별도 semantic change로 진행한다.
- 모든 `new()` call site를 분류하기 전 raw compatibility API를 제한하거나
  제거하지 않는다. 위치 기반 reconciliation을 공통화하고 origin metadata를
  matching key에서 분리한 뒤 `ExecutedSourceSlice`를 도입한다.

- `Primitive`, `Macro`, `CompatCommand`, `Shim`, `BblParser`,
  `ScannerRecovery`, `Fallback`, `Unknown` producer를 명시한다.
- constructor가 producer/confidence를 필수로 받게 한다.
- 현재 `event_id`를 build-local `sequence`로 정직하게 versioning한다.
- whole-source scanner는 미이전 event family의 명시적 low-confidence
  compatibility bridge와 debug differential로만 동결하고 새 기능을
  추가하지 않는다.
- M13.2에서 VM이 실제 실행한 범위를 표현하는 bounded
  `ExecutedSourceSlice` interface를 먼저 고정한다. M13.6에서 family별
  whole-source bridge를 제거하고 이 interface를 유일한 production recovery
  경로로 만든다.
- mouth/expansion/execution/lowering/checkpoint/layout/rendering이 공유하는
  code/severity/provenance/recovery/phase diagnostic schema를 추가한다.

완료 조건:
- false conditional leakage는 family별 suppression test로 제거하거나 이전
  전까지 known failing characterization으로 드러내며 high-confidence
  output으로 숨기지 않는다.
- scanner recovery event는 medium/low confidence로 식별된다.
- 모든 non-fallback event 생성 경로가 producer/confidence를 명시하고
  `ExecutedSourceSlice` 경계가 file/revision/span/command/expansion을 가진다.
- sequence를 revision-stable identity로 사용하지 않는다.
- `StableEventId`는 M13.4의 token/expansion origin 이전에는 만들지 않는다.

### M13.3 Eqtb And SaveStack

상태: `in progress`

현재 구현:
- `Count`, `Dimen`, `Skip`, `Toks`, `CatCode`는 `EqKey` 기반 Eqtb와
  SaveStack assignment/restore 경로를 사용한다.
- control-sequence definition과 `\let`의 기존 layered-map storage는
  `ControlSequenceScopes` module/API 뒤로 격리됐지만 아직 Eqtb/SaveStack
  assignment owner로 이전되지 않았다. mathcode/delcode, font/box/parameter,
  old scope 제거, persistent root/state hash도 남아 있다.
- 따라서 control-sequence까지 공통 Eqtb/SaveStack을 사용한다는 이전
  status 주장은 철회하며 phase exit는 열려 있다.

이전 순서:
1. control sequence definition과 `\let`
2. count와 arithmetic
3. dimen
4. skip/muskip
5. toks
6. catcode
7. mathcode/delcode
8. font/box/parameter

모든 assignment는 공통 `assign()` API를 통과하고, local assignment는
SaveStack에 이전 값을 한 번 저장하며, `\global`/`\globaldefs`/global
arithmetic는 같은 scope resolver를 사용한다.

완료 조건:
- 모든 assignment class의 local/global/nested/restore test green
- old split register/scope restore path 제거
- Eqtb persistent root와 state hash 제공

### M13.4 Streaming Mouth And Token Origin

상태: `in progress`

현재 구현:
- production VM은 현재 catcode를 읽는 streaming `Mouth::next_token()`을
  사용하고 input continuation에 mouth cursor를 보존한다.
- file/revision-aware `TokenOrigin`과 interned expansion arena는 남아 있다.

- production `run_plain()`에서 eager whole-document `lex_plain()`을 제거한다.
- `InputStack`이 character source와 token-list cursor를 함께 관리한다.
- `Mouth::next_token()`이 현재 Eqtb catcode로 다음 token 하나만 만든다.
- `\makeatletter`/`\makeatother` lexer special case를 제거한다.
- token은 file/revision/span 또는 expansion/generated origin을 갖는다.
- 실제 expansion stack은 interned arena로 관리한다.

완료 조건:
- runtime catcode가 아직 읽지 않은 문자에 반영됨
- macro token list는 나중 catcode로 retokenize되지 않음
- input file/byte cursor와 expansion provenance가 restore 후 동일

### M13.5 Macro, Prefix, And Command Model

상태: `in progress`

현재 구현:
- full macro parameter text와 delimited argument를 보존한다.
- `\long` paragraph policy, `\protected` full-expansion deferral,
  `\outer` forbidden scanner contexts, snapshot/`\ifx` flag identity가
  실행 의미로 연결돼 있다.
- malformed-parameter structured diagnostic, expandable/unexpandable command
  split, `EngineState`/`NestFrame` 통합은 남아 있다.

- macro가 parameter count가 아니라 full parameter text와 replacement
  token-list ID를 보존한다.
- delimited argument와 balanced/runaway scan을 구현한다.
- `\long`, `\outer`, `\protected`, `\global`을 prefix scanner가 모은다.
- expandable/unexpandable command를 분리한다.
- core TeX, semantic adapter, compatibility command를 별도 variant로 둔다.
- Eqtb/input/expansion/condition/nest/alignment/page/font/aux/io/compat를
  하나의 `EngineState`로 묶는다.
- vertical/internal-vertical/horizontal/restricted-horizontal/math
  `NestFrame`을 두어 paragraph와 math boundary를 실행 상태로 결정한다.

완료 조건:
- delimited/long/protected/outer/global-prefixed fixture green
- `\edef`, `\noexpand`, `\unexpanded` behavior 고정
- compatibility 명령 추가가 core primitive match를 계속 키우지 않음
- text/space/paragraph/math boundary가 explicit mode를 사용

### M13.6 VM-Owned SemanticSink

상태: `in progress`

현재 구현:
- text/space/paragraph, math 일부, heading, citation/reference/label/link,
  environment/list, page break, float/caption/graphic, table 일부가 VM 실행
  event로 scanner recovery를 대체한다.
- footnote slice는 `\footnote`, `\footnotemark`/`\footnotetext`,
  `\tablefootnote`까지 완료됐다. 본문 inline/math event를 transaction으로
  보존하고, detached mark identity와 snapshot/replay를 함께 검증한다.
- front-matter slice는 `\title`, `\author`, `\date`, `\maketitle`뿐 아니라
  `\affil`/`\affiliation`/`\institute`, `\email`, `\keywords`, `\pacs`까지
  VM 실행 event로 이전됐다. article/mini-kernel/authblk/LLNCS/REVTeX/WACV
  bridge, conditional, alias, override, macro-expanded author separator/note,
  provenance, continuation replay, compact IR/display-list golden을 함께
  검증한다.
- ICML slice는 `\icmltitle`, 두 인자 `\icmlauthor`/`\icmlaffiliation`/
  `\icmlcorrespondingauthor`, `\icmlkeywords`,
  `\printAffiliationsAndNotice`를 실행 기반으로 처리한다. `icmlYYYY.sty`는
  preview shim으로 격리하며 affiliation label을 visible text로 내보내지
  않고 conditional/macro/override, snapshot replay, IR/display-list 경계를
  검증한다.
- direct bibliography slice는 실행된 `thebibliography` 안의 `\bibitem`과
  mini-kernel `\latexdbibitem` bridge까지 완료됐다. conditional, macro,
  alias, override, environment depth, nested semantic capture, invocation/key
  provenance, continuation snapshot, compact IR/display-list golden을 함께
  검증한다.
- legacy `\bibliography`와 optioned `\printbibliography`도 실제 실행된
  occurrence만 local `jobname.bbl`을 읽고 구조화된 bibliography event를
  만든다. false conditional/override에서는 `.bbl`을 읽지 않으며 macro
  provenance, input dependency, occurrence ordering, jobname snapshot/restore를
  검증한다.
- non-visible bibliography metadata slice는 `\addbibresource`,
  `\bibliographystyle`, `\nocite`, `\defcitealias`의 실제 실행이 옵션과
  인자를 소비하도록 이전됐다. direct/macro/`\let` alias, user override,
  continuation replay, IR/display-list 비누출을 검증한다. 이 slice는 실행
  및 visibility 경계만 소유하며 resource 등록, style 선택, `nocite`
  inclusion, citation alias의 semantic aux 모델링은 아직 남아 있다.
- bibliography punctuation/delimiter slice는 `\addcomma`, `\addcolon`,
  `\addsemicolon`, `\adddot`, `\adddotspace`, `\isdot`, dash/slash helper,
  `\bibopen...`/`\bibclose...`를 VM 실행 text로 이전했다. macro/`\let`,
  conditional, override, continuation replay, IR/display-list와 literal
  brace 보존을 검증한다.
- explicit bibliography spacing slice는 `\addspace`, `\addabbrvspace`,
  `\addnbspace`, `\addthinspace`, `\addlowpenspace`, `\addhighpenspace`를
  VM 실행 space로 이전했다. punctuation의 `attach_next` 정책과 raw-source
  whitespace gap, capture on/off legacy output 일치도 함께 검증한다.
- bibliography state-helper visibility slice는 `\newunit`, `\finentry`,
  `\unspace`, `\nopunct`, `\urlprefix`의 space/no-output behavior를 VM
  실행으로 이전했다. 이 단계는 raw command 비누출만 보장하며 biblatex
  punctuation tracker와 conditional punctuation fidelity는 아직 남아 있다.
- common one-argument bibliography wrapper slice는 `\mkbibquote`,
  `\mkbibparens`, `\mkbibbrackets`, `\mkbibbraces`, common style/name
  wrapper, `\mkbibsuperscript`/`\mkbibsubscript`, `\enquote`, `\parentext`를
  VM 실행으로 이전했다. optional star, nesting, macro/`\let`, conditional,
  override, continuation replay, IR/display-list를 검증한다. style과
  super/subscript는 현재 visible text 및 attachment만 보존하며 구조화된
  typography/layout 의미는 아직 만들지 않는다.
- bibliography string slice는 `\bibstring`의 실제 실행이 optional star와
  완전 확장된 key를 소비하고 `andothers`를 `et al`로 lookup하도록
  이전했다. unknown key의 readable fallback, capture on/off, macro/`\let`,
  conditional, override, continuation replay, IR/display-list를 검증한다.
- bibliography field-wrapper visibility slice는 `\bibinfo{field}{value}`와
  `\bibfield{field}{value}`의 실제 실행이 field selector를 확장 없이
  소비하고 value token list만 재실행하도록 이전했다. capture off,
  nested wrapper, macro/`\let`, conditional, override, continuation replay,
  IR/display-list와 기존 aux citation-field regression을 검증한다. field
  metadata를 VM semantic event로 만드는 작업은 아직 남아 있다.
- bibliography identifier visibility slice는 `\doi{value}`와
  `\eprint{value}`를 transparent one-argument VM wrapper로 이전했다.
  capture off, nested wrapper, macro/`\let`, conditional, override,
  continuation replay, IR/display-list와 DOI/eprint aux citation regression을
  검증한다. hyperlink annotation과 identifier semantic node는 아직 없다.
- natbib year-suffix visibility slice는 `\natexlab{value}`와
  `\NAT@exlab{value}`를 attached transparent VM wrapper로 이전했다.
  `@`가 letter인 정상 tokenization과 raw `.bbl`에서 `\NAT` + `@exlab`로
  분리되는 기본 catcode 경로를 모두 실행하며, `\newblock`과 함께 raw
  markup이 출력에 남지 않게 한다. capture off, macro/`\let`, conditional,
  override, continuation replay, IR/display-list, internal compiler smoke를
  검증한다. 저장된 `.bbl` source는 replay/debugging을 위해 원문 그대로
  유지하고 실행 output과 semantic aux projection만 정규화한다.
- phantom visibility slice는 `\phantom{...}`, `\hphantom{...}`,
  `\vphantom{...}`을 typed VM primitive로 이전했다. 실제 실행된 인자를
  보이지 않게 소비하고 invocation 전체를 suppression range로 기록해
  내부 citation/reference/math scanner recovery event도 남기지 않는다.
  capture on/off, macro/`\let`, conditional, override, input-boundary replay,
  mini-kernel snapshot/mounted input, IR/display-list, focused production
  smoke를 검증한다. 현재 단계는 visibility만 소유하며 phantom box의
  width/height/depth와 인자 내부 TeX side effect는 아직 Layout IR로
  모델링하지 않는다.
- bibliography case-wrapper visibility slice는 `\NoCaseChange{...}`,
  `\MakeSentenceCase{...}`, `\MakeTitleCase{...}`와 optional star를
  transparent VM wrapper 실행으로 이전했다. capture on/off, raw `.bbl`,
  macro/`\let`, conditional, override, input-boundary replay,
  IR/display-list, focused production smoke를 검증하고 인접한 source text를
  임의로 띄우지 않는다. 이 단계는 wrapper 실행과 visibility만 소유하며
  실제 sentence/title case 변환과 locale-aware capitalization은 아직
  구현하지 않는다. 저장된 `.bbl`은 원문 그대로 유지한다.
- no-output state-helper slice는 `\leavevmode`와 `\unskip`을 typed VM
  primitive로 이전했다. `\leavevmode`는 command leakage 없이 실행되고,
  `\unskip`은 현재 legacy linear output, 직전 executed `Space` event,
  구조화된 table cell의 trailing whitespace를 제거한다. capture on/off,
  raw `.bbl`, macro/`\let`, conditional, override, input-boundary replay,
  IR/display-list, focused production smoke를 검증한다. 이는 아직 실제
  horizontal-mode 진입이나 hlist glue 제거가 아니며, checkpoint 전에 이미
  외부화된 output prefix를 다시 수정하지 않는다.
- bibliography box-wrapper slice는 `\framebox`, `\makebox`, `\raisebox`,
  `\parbox`를 서명별 typed VM primitive로 이전했다. bracket형 옵션과
  치수 인자를 비가시 metadata로 소비하고 body만 정상 token queue에서
  실행하며, `framebox/makebox`의 picture-mode `(width,height)[position]`
  형식도 처리한다. mini-kernel의 부정확한 wrapper macro는 제거했다.
  capture on/off, raw `.bbl`, macro/`\let`, conditional, override,
  input-boundary replay, IR/display-list, focused production smoke를 검증한다.
  실제 box geometry, alignment, raise, dimension evaluation은 아직 LayoutIr
  의미가 아니다.
- visible text-symbol slice는 `\textquotesingle`, `\textquotedbl`,
  `\textless`, `\textgreater`, `\textbar`, `\slash`를 canonical name과
  visible character를 가진 typed VM primitive로 이전했다. control-word
  뒤 source whitespace를 임의로 복원하지 않아 다음 문자에 올바르게
  붙인다. capture on/off, raw `.bbl`, macro/`\let`, conditional, override,
  input-boundary replay, IR/display-list, focused production smoke를 검증한다.
  이는 font encoding이나 glyph-level symbol layout을 모델링하지 않는다.
- text-script wrapper slice는 `\textsuperscript{...}`와
  `\textsubscript{...}`를 typed VM primitive로 이전했다. visible body는
  정상 token queue에서 실행하고 outermost wrapper depth와 pending
  word-boundary state를 snapshot에 보존해 연속 script는 붙이고 뒤의
  단어만 분리한다. mini-kernel의 compatibility macro는 제거했으며 사용자
  정의는 계속 builtin보다 우선한다. capture on/off, raw `.bbl`,
  macro/`\let`, conditional, override, active-wrapper input-boundary replay,
  IR/display-list, focused production smoke와 exact-output composite smoke를
  검증한다. 이는 visible text attachment만 소유하며 실제 script
  typography와 baseline shift는 Math/LayoutIr 작업이다.
- full biblatex localization table과 capitalization/plural variant,
  package-specific multi-argument/style bibliography wrapper, 아직
  bridge되지 않은 profile command, 일부 math/table/wrapper text는 scanner
  recovery이며 family별 이전을 계속한다.

M13.4의 file-aware token origin과 expansion record로 `StableEventId`를 먼저
도입하고 sequence와 분리한다. stable anchor는 file/token fingerprint,
expansion chain, semantic role, local ordinal을 사용하며 byte offset
단독 hash는 금지한다.

이벤트 이전 순서:
1. text/space/paragraph
2. inline/display math boundary
3. heading/title
4. citation/reference/label/link
5. environment/list/footnote/direct bibliography
6. float/caption/graphic
7. table/alignment
8. generic/class/ICML profile metadata
9. `\bibliography`/`\printbibliography` materialization
10. non-visible bibliography metadata execution
11. bibliography punctuation/delimiter execution
12. explicit bibliography spacing execution
13. bibliography state-helper visibility execution
14. common one-argument bibliography wrapper execution
15. bibliography string lookup execution
16. bibliography field-wrapper visibility execution
17. bibliography DOI/eprint visibility execution
18. natbib year-suffix visibility execution
19. phantom wrapper visibility execution
20. bibliography case-wrapper visibility execution
21. no-output state-helper execution
22. bibliography box-wrapper visibility execution
23. visible text-symbol execution
24. text-script wrapper attachment execution

각 slice는 conditional/macro-generated divergence test, actual execution emit,
실제 expansion provenance, legacy scanner differential, production switch,
scanner rule demotion 순서로 직접 완료한다.

완료 조건:
- `run_plain()`이 whole-source scanner event를 authoritative stream으로
  사용하지 않음
- replay/cancel에서 event 중복 없음
- 모든 production event가 producer/confidence/dependency를 명시

### M13.7 Snapshot V2

상태: `in progress`

현재 구현:
- semantic sink state, active capture cursor, input continuation과 event prefix가
  snapshot/replay에 포함된다.
- 완전한 `ContinuationCheckpoint`, persistent roots, diagnostic/dependency
  transaction cursor는 남아 있다.

- safe preamble 전용 `FormatSnapshot`
- 완전 실행 재개용 `ContinuationCheckpoint`
- Eqtb/save/input/conditional/expansion/nest/alignment/page/font/aux/io state
- dependency/event/diagnostic cursor와 next stable ID state
- sink `mark/rollback/commit`
- persistent Eqtb/token/font/aux root

가장 중요한 gate:

```text
full build
==
checkpoint restore + replay
```

비교 대상은 event, SemanticDocumentIr, diagnostics, dependency trace, aux,
LayoutIr, PageDisplayList, write state다.

### M13.8 SemanticDocumentIr V3 And LayoutIr

상태: `planned`

- `DocumentIr`의 역할을 semantic IR로 명시하고 migration alias를 허용한다.
- 모든 node에 `IrMeta`/stable `NodeId`/event range/semantic hash/dependency를
  둔다.
- heading/caption/bibliography/table cell/link content를 공통
  `InlineContent`로 보존한다.
- inline/display math는 VM이 만든 같은 `MathFragment`를 사용한다.
- builder의 여러 active field를 하나의 frame stack으로 통합한다.
- builder가 diagnostics와 event-to-node map을 함께 반환한다.
- 별도 `LayoutIr`에 glyph/HBox/VBox/rule/glue/kern/penalty/discretionary/
  insert/mark/link/image node를 둔다.

완료 조건:
- nested inline semantics가 문자열로 평탄화되지 않음
- builder mismatch가 구조화 진단을 냄
- SemanticDocumentIr와 hlist/vlist LayoutIr 경계가 분리됨
- replayed event stream에서 같은 semantic IR을 재구축할 수 있음

## M13.B: Actual Browser Output Lane

이 lane은 의미 기반 재구축과 독립적으로 즉시 진행할 수 있다.

### M13.B0 PDF Bootstrap

- WASI가 이미 만든 `output.pdf`를 PDF.js 또는 iframe fallback으로 표시한다.
- `extracted_text` 48줄과 fake `wasi-page-*` visible preview를 제거한다.
- failed build는 last-good PDF를 유지한다.

### M13.B1 Compiler-Owned Pages

- `/workspace/pages.json`과 `/workspace/build-meta.json`
- compiler-owned page ID/order/size/hash/source span/diagnostics
- browser asset manifest
- browser가 ID나 geometry를 생성하지 않음

### M13.B2 Display-List Page Renderer

- `P0.3a` implemented/focused-green:
  - schema v2 `BrowserFontAsset`과 normalized glyph command
  - bundled face의 used-glyph deduplication과 bounded Type1 outline extraction
  - exact identity/coverage가 맞는 run의 positioned SVG path
  - known-empty whitespace와 missing glyph를 구분하고 mismatch/missing 시
    whole-run CSS fallback
  - WASI PDF의 bundled TeX font selection
  - deterministic raw/gzip/Brotli budget과 SHA-256 identity
  - 별도 명령으로 환경을 기록하는 fresh-process Node compile 표본
- Canvas/SVG가 positioned glyph/rule/image/link op를 소비
- changed page만 교체
- exact bundled outline font 사용
- PDF.js는 final-output 비교 및 fallback으로 유지

완료 조건:
- 브라우저와 PDF page count/geometry/content 일치
- 수식/표/그림 fixture screenshot이 실제 조판 결과를 검증
- preview 문제와 compiler/layout 문제를 분리해 진단 가능

## M14: Native TeX Math

M14는 M13.3~M13.6의 실행 기반과 font substrate 위에서 시작한다. 상세
모델과 fidelity gate는
[`docs/math-rendering-plan.md`](/home/seorii/dev/hancomac/latexd/docs/math-rendering-plan.md)
를 따른다.

### M14.0 Hermetic Classic Fonts

- native `KpathseaFontResolver`와 WASI `BundledFontResolver`
- license-audited `cmr`, `cmmi`, `cmsy`, `cmex` manifest
- TFM/VF width/height/depth/italic/lig-kern/next-larger/extensible/math params
- native/WASI face/metric parity

폰트 resolver와 metric parser는 M13 core lane과 파일 소유권이 겹치지
않으므로 먼저 병렬 진행할 수 있다.

### M14.1 VM-Owned MathList

- inline/display 공통 noad/math item model
- mathcode/delcode/family/slot
- 8개 style
- scripts/fraction/radical/accent/fence/choice/glue/kern/box/alignment/error
- 실제 VM expansion provenance와 local recovery

선행조건: M13.3~M13.6 완료.

### M14.2 Fixed-Point Math Layout

- atom class와 Bin-to-Ord
- 전체 spacing table
- scripts/fractions/operators/radicals/accents/extensible delimiters
- glyph/rule/axis/source mapping PageDisplayList
- formula crop registered IoU `>= 0.90`와 geometry tolerance

선행조건: M14.0과 M14.1 완료.
추가 선행조건: M13.8의 renderer-neutral LayoutIr boundary.

### M14.3 LaTeX/AMS Structures

- matrix/cases/array/aligned/gathered/split/align/equation tags
- 공통 alignment/box 엔진
- 기능 단위 package compatibility matrix
- no-op shim을 지원으로 집계하지 않음

### M14.4 Unicode/OpenType MATH

- MATH constants, accents, math kern, variants/assemblies
- logical text/glyph ID/cluster
- XeTeX/LuaTeX/`unicode-math` 별도 profile
- renderer backend는 layout 결정을 하지 않음

## M14.I: Browser Incremental Lane

### M14.I0 Worker Isolation

M13.B0 이후 병렬 가능:
- Web Worker
- revision ordering/cancellation/coalescing
- last-good preview

이 단계는 UI isolation이며 semantic incremental로 표시하지 않는다.

### M14.I1 Persistent CompilerSession

선행조건: M13.7 Snapshot v2 equivalence.

- one-shot WASI `_start` 대신 persistent session ABI
- project/VM/checkpoint/dependency/page/font/aux ownership
- preamble/shipout/input boundary safe checkpoints

### M14.I2 Replay And Stable Tail

- VM semantic-state hash
- page/display-list hash
- dependency read set
- aux/write/mark/float state
- engine/profile/font/layout schema key

모두 같을 때만 tail을 재사용한다. page hash 단독 일치는 충분하지 않다.

## M15: Document Fidelity

- 일반 HBox/VBox/rule/glue/kern/penalty/insert/mark/whatsit
- math array와 text table이 공유하는 alignment 엔진
- `tabular`, `\halign`, span/rule/width distribution, long-table page split
- graphics size/trim/viewport/clip/rotation과 browser-safe asset 처리
- color/link/destination/annotation
- footnote/float/column/page penalty/output-routine compatibility
- visible behavior 기반 package compatibility corpus

WASI에서 외부 변환기가 필요한 형식은 명시적으로 진단하고 native-only
성공을 browser parity로 집계하지 않는다.

## Direct Commit Order

1. `P0.3a` closeout evidence와 truthful plan rebaseline
2. V2 exit criterion을 path/test별 green/red/missing으로 고정
3. scanner RawFallback/Diagnostic characterization과 opaque `EventOrigin` write
   boundary 도입 — `f06bcdf` landed
4. 기존 migrated family를 typed origin으로 이전하고, `new()` call site를
   실제 producer/consumer별로 분류 — 현재 production 0개/test call 97개
   inventory 완료
5. production family typed migration과 layered static guard는 완료
   (`776d604`, `75a79d5`). 24개 constructor contract test는 호환 API가 있는
   동안 유지하고 73개 incidental fixture를 origin별로 이전; serialized
   `Command`, lossy `Fallback`/low 의미는 별도 audit 전 유지
6. 7개 family의 location-only overlap 공통화와 origin metadata 분리는
   `decccd7`에 landed. producer-coupled anchor/sequence reuse를 audit한 뒤
   bounded `ExecutedSourceSlice`를 도입
7. control-sequence behavior characterization — 기존 grouping/global/
   `\globaldefs`/snapshot tests green
8. 기존 layered scope representation을 semantics 변경 없이 bounded
   `ControlSequenceScopes` module/API로 격리 — `94d277e` landed
9. V2 gate가 green이거나 storage migration과 무관함이 증명된 뒤에만
   control-sequence definition/`\let`을 Eqtb/SaveStack 단일 owner로 이전
10. remaining assignment class와 old split scope 제거
11. streaming Mouth의 file/revision-aware TokenOrigin과 expansion arena
12. macro/command boundary를 유지하며 EngineState와 execution mode/nest 통합
13. V6 whole-source scanner retirement와 실행 기반 SemanticSink exit
14. token/expansion 기반 stable event ID
15. Snapshot v2와 transactional replay
16. SemanticDocumentIr metadata/frame builder와 LayoutIr
17. MathList/수식 layout/AMS/OpenType 순서

8번은 전체 52,200줄 split 완료를 한 batch로 요구하지 않는다. 즉시 필요한
control-sequence state family부터 기계적으로 격리하고, backing ownership을
바꾸는 9번과 별도 work unit/rollback boundary로 유지한다. 새 V6/V7 feature
family는 이 dependency spine이 닫힐 때까지 동결하고 기존 slice는
characterization evidence로 보존한다.

브라우저 PDF/pages와 font resolver는 disjoint file lane에서 병렬 진행할 수
있다. persistent session은 Snapshot v2보다 먼저 시작하지 않는다.

## CI Gates

현재 `.github/workflows/ci.yml`의 실제 gate는 stable Rust/web/E2E, external
component diff, push-only licensed corpus와 renderer benchmark다. 아래 목록은
현재 gate와 future target을 함께 적은 것이며, 구현되지 않은 StableEventId,
MathList, persistent session용 항목을 현재 CI가 이미 보장한다고 해석하지
않는다.

### Default

- VM module unit/characterization tests
- local/global assignment와 catcode/input provenance
- macro parameter/prefix expansion
- event producer/confidence/sequence goldens과 origin 도입 뒤 stable-ID goldens
- event-to-IR mapping과 builder recovery
- format/continuation snapshot equivalence
- browser PDF/pages artifact
- bundled ten-face outline audit와 actual WASI PDF font-resource parity
- compact font/native-WASI parity

### Push

- broader VM/IR/checkpoint/internal compiler tests
- conditional/macro/package interaction
- selected AMS/alignment/package fixtures
- external-engine differential artifact
- browser source-span/E2E checks; screenshot baseline은 아직 future target

### Nightly/Manual

- 현재 scheduled workflow는 없으며 다음은 future target이다.
- randomized group/assignment/macro/math grammar
- 모든 safe boundary checkpoint/replay stress
- licensed paper corpus와 CC0 corpus
- Unicode/OpenType profile
- multi-revision performance/cache sweep

## Parallelization Lanes

### Lane A: VM Core

순차 전용:
- `tex-vm` module split
- Eqtb/SaveStack
- Mouth/expansion/macro
- SemanticSink
- Snapshot

같은 VM module family를 두 작업자가 동시에 수정하지 않는다.

### Lane B: Browser Delivery

M13 core와 병렬 가능:
- `web/apps/viewer`
- `crates/latexd-wasi`
- PDF/pages/display-list browser adapter

### Lane C: Font Substrate

M13 core와 병렬 가능:
- `tex-fonts`
- hermetic font manifest와 fixture

### Lane D: Event/IR

VM event family가 준비된 순서대로 제한적 병렬:
- `tex-render-model`
- `tex-layout/document_ir_builder`
- event/IR goldens

### Lane E: Math Layout And Oracle

M14.0/M14.1 이후:
- `tex-math-model`
- `tex-math-layout`
- math atlas/differential harness

## External Oracle Lane

- pdfTeX/Tectonic은 classic execution/geometry oracle
- XeTeX/LuaTeX은 Unicode/OpenType oracle
- engine/package/font version을 manifest에 고정
- external final output과 internal fast preview를 UI/artifact에서 명시
- reference source는 파일별 라이선스 확인 전 복사하지 않고, 기본은
  behavioral oracle와 독립 구현

## 금지 방향

- whole-source scanner pattern을 production semantics로 계속 확장
- eager lexing을 유지한 채 runtime catcode를 흉내냄
- register locality를 type별 map에서 따로 수정
- `\long`/`\outer`/`\protected`를 no-op으로 유지
- scanner event에 high confidence 자동 부여
- event sequence나 byte offset을 stable identity로 사용
- input/conditional/nest/sink cursor 없는 불완전 snapshot
- sink transaction 없이 replay
- MathList를 execution-generated event보다 먼저 구현
- `normalized_text`를 수식 배치에 사용
- semantic IR과 hlist/vlist LayoutIr를 합침
- 매 요청 fresh VM을 incremental로 표기
- package no-op shim을 지원으로 집계
