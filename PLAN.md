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

- 기준 시점: `2026-08-12`
- 기준 commit: `c0c6c9e` (`docs: record overpic overlay suppression evidence`)
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
- control-sequence root/current definition, visible lookup, group depth와
  snapshot layer 변환은 Eqtb/SaveStack이 소유한다. 기존
  `ControlSequenceScopes` module은 제거됐고 serialized `scopes` schema는
  Eqtb+restore history 투영으로 유지한다 (`775cc22`).
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
  `9fef0b7`, `9d122b0`, `69e75ea`에 landed했다. 임시 raw 호환 API였던
  `with_origin()`과 `new()`는 전체 call-site 분류 뒤 함께 제거됐다 (`0940368`).
  공개 Rust 호출자는 일반 event에 `try_from_origin()`, scanner 복구에
  `from_scanner_recovery()`, 필요하면 마지막에 `with_mode_hint()`를 사용한다.
- production `src/`에서는 legacy 생성자 호출과 직접 metadata 변경을 syntax
  test로 차단하고, workspace lib/bin에서는 기존 Clippy `disallowed-methods`를
  방어선으로 유지한다 (`776d604`). 구조 정책은 공개 associated constructor를
  `try_from_origin()`/`from_scanner_recovery()`로 제한하고, private
  `from_metadata()` 호출도 `try_from_origin()`에서만 허용한다 (`0940368`).
  table raw-fallback 승격과 text leading-space 재조정은 sequence/source/mode를
  보존한 채 typed origin으로 envelope를 재구성하도록 이전됐다 (`75a79d5`).
- list, environment, heading, caption, graphic, front-matter, bibliography의
  동일한 full-provenance overlap은 `source_locations_overlap()`으로 공통화됐다
  (`decccd7`). matching identity는 primary/related/expansion의 half-open file
  span만 사용하고 `generated_by`, producer/confidence, truncation은 보지 않는다.
  기존 호환 동작대로 모든 related role과 expansion definition span도 포함한다.
  span 규칙이 다른 inline/text/footnote는 이 mechanical batch에서 제외했다.
- heading, caption, graphic, front-matter의 동일한 unmatched insertion anchor는
  terminal expansion call, related Invocation, primary 순서를 유지하면서
  source-only `call_invocation_primary_anchor()`로 이전됐다 (`694a0ee`). permissive
  legacy deserialize에서 source가 같고 producer만 다른 heading도 같은 위치에
  삽입되며 envelope metadata/payload는 보존된다. bibliography의 expansion→primary
  anchor와 graphic candidate의 producer-dependent path equivalence는 별도 의미
  변경이므로 유지한다.
- lexical false branch뿐 아니라 runtime `\ifnum` false branch의 table
  scanner/fallback event도 executed suppression range로 제거한다. 판정은
  table 시작 anchor에 한정해 cell 내부 phantom/spacing suppression이 visible
  table 전체를 제거하지 않는다.
- runtime-false `minipage`의 scanner-only `BeginLayoutContainer`/
  `EndLayoutContainer`도 environment reconciliation family에 등록해 기존
  suppression range로 제거한다 (`e69cb6d`). visible minipage의 layout pair와
  본문은 유지된다.
- runtime-false `DocumentClass` recovery도 같은 suppression-aware structural
  family에서 제거하며 실제 class와 본문은 유지한다 (`7e24b0a`). Package-derived
  `SetDocumentLayout`도 같은 suppression family에 들어가고, source scan 중의
  class-option in-place mutation은 제거했다. Reconciliation은 suppression을
  통과한 scanner NeurIPS layout만 앞선 surviving class에 `10pt`로 투영하므로
  runtime-false package는 event, class options, Document IR layout을 오염시키지
  않는다 (`f37fd97`, resolved `BUG-025`).
- scanner의 `\twocolumn` / `\onecolumn`도 class options를 즉시 바꾸는 대신
  source-scoped `SetDocumentLayout`을 만든다. Column-command event ID는 semantic
  environment snapshot, recovery refresh, event-ID remap에 함께 보존하고,
  reconciliation을 통과한 event만 앞선 surviving class의 column option으로
  투영한다. 따라서 runtime-false command는 class/IR layout을 바꾸지 않으며
  visible command와 continuation replay는 동일한 column count를 유지한다
  (`a3a39a0`).
- runtime-false `graphicx` package-mode option은 이후 visible graphic의 scanner
  payload에 남을 수 있었다. Direct primitive reconciliation은 scanner에만 붙은
  `draft`/`final`/`demo` 등 package-mode prefix만 execution-owned payload에서
  제거하고, resize/scale 같은 scanner wrapper option은 계속 보존한다
  (`126f383`). Scanner-only graphic에 적용되는 `Gin` default도 source-scoped
  contribution과 execution occurrence를 snapshot에 보존한다. Reconciliation은
  runtime-false contribution만 brace-aware option sequence에서 제거하고 visible
  default, 동일 값의 별도 contribution, local wrapper option을 유지하며 input-exit
  replay와 recovery refresh의 event-ID remap도 보존한다 (`d5714b7`).
- Pro review 뒤 graphic path/extension/package state를 위한 generic scanner mutation
  log는 만들지 않기로 했다. Loaded `overpic`의 실제 `\begin` 실행 경계가 local
  option과 backing path를 소비하고 현재 VM graphic path/extension/default state로
  primitive event를 만든다. 따라서 runtime-false extension과 arbitrary package
  default는 보이는 `overpic`에 도달하지 않고, visible state는 유지되며 package
  shim 없이 같은 이름을 쓴 환경 본문도 소비하지 않는다 (`533d7ee`). 새 scanner
  state family가 event 존재 여부, package loading, pending-option routing 또는
  cross-command deferred semantics를 재현해야 하면 contribution ledger를 늘리지
  않고 bounded `ExecutedSourceSlice`로 wrapper boundary를 이전한다.
- missing input/package/class와 cyclic input에서 만들어지는 scanner
  `RenderDiagnostic`도 같은 suppression-aware family에 등록했다. Runtime-false
  missing input은 diagnostic event를 남기지 않고, visible missing input은 기존
  `Unknown`/low event를 유지하며 input-exit continuation도 같은 결과를 replay한다
  (`2348ff5`).
- non-table `RawFallback`도 suppression-aware environment family에 등록해
  runtime-false unknown environment의 scanner recovery가 남지 않게 했다. Table
  fallback은 cell 내부 phantom/spacing suppression이 visible table 전체를 지우지
  않도록 기존 table-start anchor 전용 경로를 유지한다 (`01b3634`).
- algorithmic `\If`/`\Else`/`\Comment`가 직접 만드는 scanner `Text`/`Space`도
  invocation-aware scanner slot에 등록했다. 따라서 runtime-false 명령의 prefix,
  suffix, note text, explicit space는 제거되고 뒤의 visible 명령은 한 번만 남는다
  (`cba90eb`). 이후 전체 producer 감사에서 찾은 theorem optional title, inline
  formatting/link/unit helper, escaped space/symbol, `\xspace`, overpic overlay,
  nested fallback, color/float text와 recursive-input EOF Space까지 모두 bounded
  ownership으로 이전됐다. EOF Space는 parent input invocation이 소유해 skipped
  occurrence만 제거하고 TeX endline spacing과 changed-child replay를 보존한다
  (`d2e4170`, `ffe6d19`). Manual `Text`/`Space` writer inventory는 닫혔다.
- runtime-false prefix와 visible lossy caption을 같은 macro invocation argument에
  둔 회귀도 추가했다. 이 형태에는 scanner caption이 없으므로 실행이 도달한
  `Fallback`/low caption이 유일한 증거다. Executed caption은 이미 VM control
  flow를 통과했으므로 coarse source suppression을 다시 적용하지 않고 보존하며,
  실제 false branch 안의 macro caption은 계속 생성하지 않는다 (`4520dfd`).
- repeated input의 structured label과 normal/lossy caption은 global scanner event
  anchor로 parent invocation occurrence를 구분한다. Skipped occurrence는 제거하고
  visible occurrence만 execution event와 reconcile하며, 두 visible input은 두
  label을 유지한다. Changed-child input-enter replay는 clean run과 같고, completed
  link/caption이 nested event를 접을 때 dangling anchor도 남기지 않는다
  (`ffe6d19`, `99aa595`). Caption-local snapshot schema는 추가하지 않았다.
- simple inline wrapper(`\emph` 계열)의 직접 scanner text도 invocation-aware slot에
  연결했다. Runtime-false `\emph{Wrong}`은 제거되고 visible `\emph{Right}`은 한
  번만 남으며, 기존 scanner-recovery/medium event가 실행 primitive/high event로
  승격되는 provenance 계약도 golden으로 고정했다 (`829bb34`). 중첩 command를
  포함한 wrapper 분기와 별도 link/unit/symbol helper는 계속 열린 목록이다.
- theorem-like environment의 optional title `Text`와 뒤따르는 `Space`도 하나의
  scanner transaction으로 등록했다. Runtime-false theorem의 title/body/block은
  모두 사라지고 visible theorem의 title/body/block은 한 번씩 유지된다
  (`7c8f2ab`).
- siunitx `\SI`/`\qty`/`\num`/`\si`/range/angle scanner text를 command invocation
  slot에 등록했다. Runtime-false quantity/unit text는 제거되고 visible control은
  한 번 유지된다 (`6ac0dde`).
- `\hyperref`, `\hyperlink`/`\hypertarget`, `\nolinkurl`/`\path`/`\detokenize`의
  direct scanner text도 invocation slot에 등록했다. Runtime-false link label과
  URL text는 제거되고 visible helper는 한 번 유지된다. 실행 가능한 braced
  `\path`는 primitive/high로 승격되는 provenance golden도 갱신했다 (`50bd7d0`).
- escaped text symbol, explicit control-space, `\xspace`가 직접 만드는 scanner
  `Text`/`Space`도 각 invocation slot에 등록했다. Runtime-false helper events는
  제거되고 visible `%`, explicit space, `\xspace`는 각각 한 번 유지된다
  (`17d287e`).
- threeparttable `\tnote` marker text도 invocation slot에 등록했다. Runtime-false
  marker는 제거되고 visible `[marker]` text는 한 번 유지된다 (`623df34`).
- overpic `\put`/`\multiput` overlay text도 overlay invocation slot에 등록했다.
  Runtime-false overpic의 graphic과 overlay text가 함께 제거되고 visible overpic의
  primitive graphic과 overlay text는 유지된다 (`5f6b9a3`).
- phase exit는 열려 있다. 전체 112개 call site 분류와 production/fixture
  migration을 마쳤고, public raw constructor 정의와 실제 Rust call expression은
  모두 0개다 (`0940368`). origin-sensitive semantic-text fixture 3개는
  scanner medium/macro high typed origin으로 이전됐고 (`91a8daa`), synthetic
  semantic-sink fixture 2개는 unknown/low로 이전됐다 (`edbe93c`). golden text와
  compiler diagnostic fixture도 각각 unknown/low와 diagnostic-unknown으로
  이전됐다 (`dc72656`). layout fixture 63개도 ordinary synthetic event 62개는
  unknown/low, `RawFallback` 1개는 fallback origin으로 이전됐다 (`247a647`).
  model serialization fixture 6개도 unknown/low로 이전됐다 (`80ca0e2`). guard
  self-test의 parsed source string 안에 있는 legacy call 예시는 이 inventory에서
  제외한다. 고정 Macro/Medium JSON fixture는 permissive legacy read가 생성자
  제거 뒤에도 유지됨을 검증한다. bibliography unmatched insertion은
  expansion→primary source geometry로 producer와 분리됐다 (`4c24516`). Footnote
  identity도 event sequence correspondence 없이 changed-source refresh 뒤 clean
  build와 같은 allocation phase로 rebase하며, repository audit은 남은 sequence
  사용이 build-local ordering/transaction correlation임을 확인했다 (`ba9424d`,
  `2bc5154`). graphic의 producer-coupled path equivalence는 현재 scanner wrapper
  option과 macro override 방어를 보존해야 하므로 execution identity 뒤로
  보류한다. definition span만 공유하는 반복 macro invocation의 교차 matching
  가능성도 기존 `ARCH-007`의 coarse byte-overlap risk에 속한다.
- M13.2 phase exit는 전체 recovery family의 suppression regression 또는 명시적
  low-confidence expected-failure inventory를 끝낼 때까지 열려 있다. Manual
  `Text`/`Space`, include EOF, repeated label/caption occurrence와 대표 replay는
  닫혔고 diagnostic subtype characterization도 완료됐다. `ARCH-007`의
  skipped-prefix 뒤 visible graphic은 authoritative executed lane의 coarse
  suppression을 제거하고, empty-replacement macro가 소비한 비실행 graphic
  인수는 scanner invocation range로 억제해 닫았다. bibliography는 기존
  execution-anchor reconciliation이 같은 mixed-macro 회귀를 이미 만족한다
  (`9812429`). 남은 blocker는 macro-generated `RawFallback`의 exact isolation이다.
  Pro review는 executed begin에서 source end를 미리 찾아 완성 이벤트를 만드는
  방식을 거부했다. executed end commitment, occurrence identity, dynamic
  environment classification, all-family child ownership, open-capture snapshot
  state가 갖춰질 때까지 `ARCH-007`로 유지한다. 구체
  `ExecutedSourceSlice`, file/revision/expansion identity, public event
  revision/dependency schema, shared diagnostic schema는 더 이상 M13.2 exit
  blocker로 세지 않는다. 각각 M13.4/M13.6 또는 별도 readers-first architecture
  stream의 책임이다.
- internal compiler의 path-based build dependency는 final reconciled
  `GraphicRef`/`IncludePdf`를 추적한다. 보이는 missing asset은 추적하고
  runtime-false asset은 제외한다 (`9ed7a09`). 이 read set은 cache invalidation
  선행 단위일 뿐 semantic `DependencyId`나 event identity 증거가 아니다.

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
  제거하지 않는다는 선행 조건은 fixture 이전으로 충족됐다. 후속 Pro review에
  따라 두 raw 생성자는 함께 제거했고 (`0940368`), wire field/tag와 permissive
  serde read는 바꾸지 않았다. typed constructor 경계가 sanctioned write path를
  제한할 뿐 public field/serde로 만들 수 있는 모든 표현값의 유효성을 보장하지는
  않는다.
- 후속 Pro review는 네 개의 동일 insertion anchor만 source-only로 바꾸고,
  bibliography anchor, graphic equivalence, sequence/source reuse는 각각 별도
  RED로 남기도록 결정했다. file/revision/expansion identity가 없는 임시 타입을
  `ExecutedSourceSlice`로 명명하거나 snapshot/wire에 넣지 않는다. 후속 독립
  RED는 bibliography의 valid Macro/lossy origin이 동일 provenance에서 같은
  삽입 순서를 갖도록 고정했고, bibliography 전용 expansion→primary anchor로
  닫혔다 (`4c24516`). graphic equivalence는 현재 scanner provenance에서
  안전한 변경 근거가 없어 execution identity 뒤로 보류한다.

2026-08-12 Pro review 결정:
- schema v5와 현재 Rust variant/wire tag를 유지하고, 먼저 sanctioned
  first-party production-write taxonomy만 닫는다. opaque `EventOrigin` writer
  image는 `Primitive`, `Macro`, `ScannerRecovery`, `Fallback`, `Unknown` 다섯
  variant이며 production AST guard가 `Command`, `Shim`, `BblParser` 직접
  construction과 blanket `From<GeneratedBy> for EventProducer`를 막는다
  (`ba887bf`).
- schema-v5 full-stream fixture는 `command`, `shim`, `bbl_parser`를 exact
  round-trip하고, active semantic snapshot은 세 compatibility-only producer를
  모두 거부한다. 따라서 writer closure와 legacy read/round-trip compatibility를
  별도 완료 조건으로 유지한다.
- `Command` rename, schema v6, 새 `Shim`/`BblParser` semantics는 실제
  producer/consumer invariant와 readers-first, rollback-safe migration 계획이
  함께 생길 때까지 보류한다.
- 후속 milestone review는 M13.2와 M13.4의 순환 gate를 해소했다.
  M13.2는 schema-v5 event/reconciliation baseline만 닫고, source registry,
  exact revision, lexical token origin, expansion/scoped command identity,
  readers-first snapshot capability, validated `ExecutedSourceSlice`는 이 순서로
  M13.4에서 도입한다. M13.6은 family별 consumer migration과 최종 scanner
  retirement만 소유한다.
- old path-only snapshot은 identity를 추론해 승격하지 않는다.
  `LegacyPathOnly`는 기존 동작만 유지하고 slice construction을 거부하며,
  `IdentityComplete(context)`만 같은 registry/interner/expansion context에서
  validated slice를 만들 수 있다. Fresh rebuild/rebase는 새 context를 만든다.
- M13.3은 M13.4와 sibling branch다. `ba9424d` 뒤 별도 independence proof가
  source/revision/expansion/path/span/build-rev 의존 없음, local
  `ControlSequenceId` lifetime 유지, wire/checkpoint format 불변, snapshot 및
  replay differential equivalence를 증명할 때만 bounded ownership migration을
  시작한다.
- public event identity/revision/dependency와 shared diagnostics는 별도
  readers-first architecture stream이다. singular `EventMeta` field를 미리
  가정하거나 build `rev`/`DepTrace`를 source revision/semantic dependency로
  재해석하지 않는다.

- 장기 versioned taxonomy target에는 `Primitive`, `Macro`, command/shim/BibTeX
  계열 producer, `ScannerRecovery`, `Fallback`, `Unknown`을 명시하되, 현재
  schema-v5 compatibility tag를 구현된 writer semantics로 오해하지 않는다.
- sanctioned constructor가 raw producer/confidence 쌍 대신 검증된
  `EventOrigin`을 필수로 받게 한다.
- 현재 `event_id`를 build-local `sequence`로 정직하게 versioning한다.
- whole-source scanner는 미이전 event family의 명시적 low-confidence
  compatibility bridge와 debug differential로만 동결하고 새 기능을
  추가하지 않는다.
- M13.4에서 identity-complete execution context와 validated internal
  `ExecutedSourceSlice`를 먼저 고정한다. M13.6에서 family별 whole-source
  bridge를 제거하고 이 interface 또는 별도 리뷰된 failure/dispatch-bounded
  input을 production recovery 경로로 만든다.
- mouth/expansion/execution/lowering/checkpoint/layout/rendering이 공유하는
  diagnostic schema는 dependency-neutral canonical owner와 transport adapter
  versioning을 별도 리뷰한 뒤 진행한다.

완료 조건:
- false conditional leakage는 family별 suppression test로 제거하거나 이전
  전까지 known failing characterization으로 드러내며 high-confidence
  output으로 숨기지 않는다.
- scanner recovery event는 medium/low confidence로 식별된다.
- 모든 non-fallback event 생성 경로가 producer/confidence를 명시한다.
- sequence를 revision-stable identity로 사용하지 않는다.
- `StableEventId`는 M13.4의 token/expansion origin 이전에는 만들지 않는다.

### M13.3 Eqtb And SaveStack

상태: `in progress`

현재 구현:
- `Count`, `Dimen`, `Skip`, `MuSkip`, `Toks`, `CatCode`는 `EqKey` 기반 Eqtb와
  SaveStack assignment/restore 경로를 사용한다.
- control-sequence definition, `\let`, lookup, group unwind는
  `EqKey::ControlSequence(String)`/`EqValue::ControlSequence(Box<Meaning>)`과
  SaveStack을 공통 owner로 사용한다 (`c640efb`, `775cc22`). Borrowed-name hot
  path를 위해 Eqtb 내부 map은 분리하되 assignment/restore 의미는 하나이며,
  interner-local `ControlSequenceId` lifetime은 바꾸지 않았다.
- 기존 `ControlSequenceScopes` production owner는 제거됐다. 다만
  mathcode/delcode, font/box/remaining parameter, persistent root/state
  hash가 남아 있어 phase exit는 계속 열려 있다.
- production owner 이전용 독립성 gate는 green이다. CI diff guard가 V3 owner
  file 변경을 감지하면 허용 경로 밖 production diff와 신규
  identity/provenance/persistence symbol을 거부한다 (`2289907`). VM continuation
  safety 2, semantic capture 22와 기존 `scopes` JSON shape는 decode-equivalent
  golden/restore fixture로 고정됐다 (`fe6b4df`). Nested local/global shadow,
  `\globaldefs`, group unwind, input-exit JSON checkpoint replay는 output, scope,
  events, diagnostics, transcript, registers와 visible/suppressed recovery가 clean
  run과 같음을 검증한다 (`d9cdf02`).
- 이 differential은 event capture expansion marker가
  `\globaldefs → \count251` 같은 register alias와 뒤따르는 assignment syntax
  사이를 끊는 버그도 드러냈다. Count/dimen/skip/toks register alias expansion은
  markerless로 실행해 assignment 의미를 보존한다 (`d9cdf02`). 같은 이름의
  global control-sequence assignment가 모든 pending local restore를 취소하도록
  TeX 호환 전제 동작도 먼저 고정했다 (`f66cdbf`).
- `775cc22`는 snapshot schema/version을 바꾸지 않고 Eqtb+SaveStack에서 기존
  `VmSnapshot.scopes`를 투영한다. Macro/primitive/token, fresh interner,
  open-group restore, input-exit replay, module base precedence, `aftergroup`,
  direct-global helper 계약이 green이다. `f9f23ef`는 restore chain을 모의
  unwind해 root/current/previous level 불변식을 fail-closed로 검증한다.
- Pro 리뷰 후 `5ca539e`, `b4c2695`는 두 이름과 begin/end/local/global 동작을
  depth 3, 길이 7까지 전수 생성해 매 단계의 exact layer와 visible lookup을
  독립 layered oracle과 비교한다. 같은 meaning을 여러 중첩 layer에 다시
  쓰는 경로도 실제 생성됐음을 assertion으로 고정했다. 현재 `Meaning`과
  `SnapshotMeaning`에는 `Undefined` variant가 없으므로 absence는 restore의
  `Option<EqEntry>::None`으로만 표현되고, 지원하지 않는 `undefined` wire
  meaning은 decode에서 거부된다.
- `824b51c`는 control-sequence meaning만 box해 register-only `EqValue`와 같은
  24-byte enum 크기를 유지한다. 전용 `BTreeMap<String, EqEntry>`의 borrowed
  lookup은 hot path에서 `String`을 새로 만들지 않는다.
- `b4d1500`은 author front-matter의 `and`/`thanks`를 Eqtb에서 임시 교체하지
  않고 full-expansion 호출에 lexical protection 목록을 전달한다. 이 목록은
  중첩 `\expanded`와 `\expandafter`에도 전파되며 canonical meaning과
  SaveStack history를 변경하지 않는다.
- legacy open-group snapshot에는 register restore history가 없다는 기존
  한계를 유지한다. Restore가 `scopes`로 재구성한 group frame은
  control-sequence restore만 기록하므로, restore 뒤 그 열린 group에서 새로
  할당한 register/catcode가 group 종료 시 snapshot 값으로 되돌아가는 새
  동작을 만들지 않는다 (`340ea72`). Group-end에서 이 Eqtb restore 다음에
  실행되는 source catcode-overlay cleanup은 frame map의 `retain`만 수행하고
  lookup/event/diagnostic/callback이 없어 합쳐진 control-sequence restore의
  순서 변경은 관찰 불가능하다고 audit했다.
- `99a55ae`는 root가 없는 `scopes=[]`를 restore mutation 전에 명시적으로
  거부한다. `90a7628`은 이를 `Vm::try_restore`의 typed error로 바꾸고 모든
  primitive reference도 VM/interner mutation 전에 검증한다. Persisted
  checkpoint를 받는 `tex-bootstrap`의 production restore 네 곳은 이 fallible
  API를 사용하며, `Vm::restore`는 trusted compatibility wrapper로만 남는다.
  Root-only/nested empty layer와 runtime 한계 1000, 기존 restore가 허용하던
  1001-depth empty layer는 exact round-trip해 새 depth policy를 만들지 않는다.
- `d58f4e0`은 restored legacy CS-only frame이 register predecessor를 기록할 수
  없는 경우를 `SaveDisposition::UntrackedLegacyFrame`으로 드러내고, 그 write를
  effective level 0으로 canonicalize한다. Legacy unwind 뒤 값/level이 유지되고
  이후 normal group이 같은 root predecessor를 정확히 복원한다.
- `scripts/check_v3_cross_version.py`는 owner 이전 직전 `f66cdbf`와 지정
  candidate를 별도 detached worktree에서 실제 빌드한다. `e5a630e`가 추가한
  두 번째 fixture는 `{x=R,y=G} / {x=L} / {} / {x=L,z=Z}` 네 layer에
  equal-value repeat, empty layer, absent predecessor, global cancellation을 한
  snapshot에 결합한다. 두 방향의 pre-continuation selected scope object 전체가
  같고 key/kind golden, output `LGZLGALGARGA`, 진단 0을 만족해야 한다.
- 정확한 `cd64df6` detached clean worktree에서 tex-vm lib 660, 관련 integration
  102, tex-bootstrap lib 64, Python guard/matrix 11, 양방향 두-fixture matrix,
  canonical workspace Clippy와 fmt가 모두 green이다. `cd64df6`은 author 전용
  `\expandafter` regression도 고정한다. 최종 Pro 재검토
  (`6a7c6421-1a38-83ee-af57-7dee503ceced`)는 이 bounded control-sequence
  ownership slice를 `PROCEED`(confidence 0.92)로 판정했다. 이 판정은 이후의
  unrelated semantic-suppression commit이나 M13.3 전체 완료로 확장하지 않는다.
- closeout hardening 중 production crate의 asserting `Vm::restore` 재사용을
  막는 Clippy/CI source guard는 `00c8ee3`에 landed했다. Persisted restore는
  `Vm::try_restore` 경계를 유지한다.

첫 미이전 assignment class인 muskip은 기존 snapshot에 상태와 allocation
cursor가 전혀 없어, 단순 owner 이전이 아니라 별도 readers-first migration으로
진행한다. Pro schema review `6a7c6961-a170-83ee-be0f-746c526eb3ac`는
hybrid reader-first/runtime-only 첫 단계를 `PROCEED`(confidence 0.87)로
판정했으며 durable muskip writer 활성화는 승인하지 않았다.

- `e3bec73`의 exact `00c8ee3` binary fixture는 field-only muskip을 old raw
  reader가 성공으로 읽고 상태를 버림을 증명한다. 같은 old binary는 nested
  versioned raw document를 거부하고, versioned-only checkpoint는 읽되 replay
  하지 않으며, dual-lane checkpoint는 legacy lane으로만 안전 재생한다.
- `dcbee7c`는 missing/unreadable prior checkpoint를 typed cache miss로
  정규화하고 compiler가 low-level loader를 직접 호출하지 못하게 policy test를
  추가했다. 호환되지 않는 cache는 사용자 실패가 아니라 이전 revision 탐색이나
  source rebuild로 귀결된다.
- `1d29aaa`는 writer가 없는 semantic document reader를 추가했다. 문서 format은
  `latexd.vm-snapshot`, 독립 semantic schema는 1이며 typed capability 문자열을
  header에서 state보다 먼저 검증한다. 알려진 schema의 unknown document/state
  field를 거부하고, legacy flat snapshot normalizer와 decode→fallible restore의
  mutation-free error boundary를 제공한다. 이 단계의 supported capability 집합은
  비어 있어 `eqtb.muskip.scalar-v1` 문서를 명시적으로 거부했다.
- `8d91fd3`은 checkpoint envelope schema 2를 유지하면서 reader-only
  `versioned_snapshot.document` lane을 추가했다. Internal attachment는
  none/legacy/versioned 중 정확히 하나이고, compiler와 replay selection은 lane
  provenance를 보존하는 `snapshot_for_restore()`만 사용한다. Dual lane은 semantic
  decode 전에 거부되고 unsupported/malformed/restore-invalid versioned state는
  production reuse에서 unreadable cache miss 또는 replay-unsafe로 닫힌다.
- 두 번째 Pro review `6a7c7c89-9d74-83ee-afd4-da353328b99f`는
  **REVISE**(high confidence)를 반환했다. 그 지적에 따라 정의되지 않은
  `state_hash`를 제거하고, parent serializer/save의 zero-byte·zero-filesystem
  preflight, metadata×attachment truth table, legacy write eligibility hook, 전체
  public-field caller migration, 실제 production envelope 양방향 검증을
  `8d91fd3`에 포함했다. Versioned serializer는 여전히 명시적으로 disabled이며
  active production writer는 legacy flat `VmSnapshot`만 쓴다.
- exact `8d91fd3` detached worktree에서 tex-checkpoint 72, tex-vm 668, Python
  policy/matrix 10, latexd focused 2, workspace test-target check, canonical Clippy,
  fmt가 green이다. `00c8ee3` old/new production envelope는 양방향 output `R`을
  재생하고, versioned-only는 hit, dual/unsupported/malformed envelope는 모두
  unreadable miss다.
- closure Pro review `6a7c8fce-8c90-83ee-8648-f7bbbdd8c596`는 reader-only
  phase를 `PROCEED`(confidence 약 0.87)로 닫되 다음 순서를 `REVISE`했다.
  Versioned/document writer만 disabled이고 durable legacy writer는 active이므로,
  runtime-only가 곧 replay-neutral이라는 전제를 두지 않는다. 현재
  `required_capabilities()`의 empty 구현은 자동 fail-closed 보장이 아니라 다음
  state-derived gate를 넣을 enforcement seam이다.
- `a2466c7`은 `LegacyOnly` 정책을 hard serializer backstop과
  preamble/shipout/input-boundary capture 결정에 함께 적용한다. Required
  capability가 있는 snapshot은 production save 오류로 넘기기 전에 attachment가
  suppression된다. 현재 snapshot은 모두 legacy-compatible이므로 synthetic
  capability policy RED/GREEN과 세 category source guard를 먼저 고정했으며, 실제
  muskip-bearing snapshot의 zero-byte/zero-filesystem/laundering 회귀는 첫
  capability-bearing state와 원자적으로 추가한다.
- `b809c30`은 runtime-only owner를 추가했다. `EqKey::MuSkip`과
  `EqValue::MuGlue(MuGlueScalarV1)`는 기존 `Skip/Glue`와 타입이 분리되고 모든
  local/global unwind는 공통 `Eqtb::assign`/SaveStack을 사용한다. 독립 muskip
  cursor는 skip cursor와 별도로 256에서 시작하며, muskip field가 없는 legacy
  snapshot restore는 changed skip cursor와 무관하게 이 초기값을 복원한다.
- `f899127`은 runtime owner의 complete in-memory snapshot barrier를 닫았다.
  `VmSnapshot`은 exact legacy DTO인 `LegacyVmSnapshotV1`과 독립
  `muskip_registers`/`next_muskip_register`를 함께 소유하고, legacy decode는 빈
  muskip map과 cursor 256을 복원한다. Map이 비어 있지 않거나 cursor가 256이
  아니면 state-derived `eqtb.muskip.scalar-v1` capability가 생기며, raw legacy
  serializer와 checkpoint parent preflight가 capability-bearing state의 byte 생성을
  모두 거부한다. Preamble, shipout, input-boundary capture는 attachment를
  suppression하고 source rebuild로 귀결하며, laundering normalization과 cursor-only
  state도 같은 gate를 통과한다.
- `5a7bd82`부터 legacy-compatible state hash도 정렬된 exact legacy projection을
  사용한다. Non-legacy state의 complete fingerprint는 같은 canonical legacy
  projection, ordered capability, canonical muskip map, cursor를
  length-delimited/domain-separated 입력으로 포함해 suppression된 재캡처도 상태
  변화를 구분한다. 이 fingerprint는 writer policy/lane과 무관한 semantic metadata
  identity이며 versioned wire 표현이나 wire-byte hash로 간주하지 않는다.
- Exact `f899127` detached worktree에서 tex-vm 전체, tex-checkpoint 전체,
  workspace test-target check, canonical Clippy/fmt, Python guard, 실제
  `00c8ee3` old/new binary migration matrix가 green이다. Candidate legacy
  bundle/envelope와 baseline envelope는 output `R`을 안전 재생하고,
  versioned-only는 hit, dual/unsupported/malformed는 unreadable miss다.
- Complete-snapshot Pro review
  `6a7ca2a2-afa4-83e8-aad8-f7381d3e7695`는 이 범위를
  `PROCEED`(confidence 0.82)로 판정했다. 후속 `f02a4cd`는 legacy DTO의 unknown
  field를 fail-closed로 거부해 예약 muskip field가 legacy lane에서 유실되는 것을
  막고, cursor 256 미만을 interner mutation 전에 거부한다. Cursor-only suppression과
  기존 legacy attachment를 suppression 재캡처가 실제 파일에서 제거하는 회귀도
  production loader/save 경계에 고정했다.
- Public `LegacyVmSnapshotV1`로의 명시적 projection은 의도적으로 lossy할 수 있다.
  Production persistence는 `VmSnapshot` 정책 경계만 사용하므로 현재 repository
  safety blocker는 아니지만, public API 안정화나 versioned capability 지원 전에는
  projection을 제한하거나 손실 계약을 명시해야 한다. 현재 이름이 넓은
  `normalize_legacy_vm_snapshot`도 그때 versioned normalization과 분리한다.
- `6604eb7`의 source-level muskip slice는 `\newmuskip`, `\muskipdef`, raw `\muskip`,
  markerless register alias, `\the`, local/global/`\globaldefs`, 그리고
  `\advance`/`\multiply`/`\divide`를 typed owner에 연결한다. Scalar-v1 범위는 base
  `mu`만 보존하며 명시적 nonzero `plus`/`minus` component는 전체 RHS를 소비하되
  기존 값을 바꾸지 않는다. 음수 index, 0 divisor, `MIN / -1`, allocator 고갈은
  wrap/panic/후속 prefix 또는 `\afterassignment` 누수 없이 종료한다.
- Alias-only/deferred state는 `eqtb.muskip.alias-v1`을 요구한다. Visible scope뿐 아니라
  모든 serialized scope history, token register, aftergroup/afterassignment/end-document
  hook, continuation token, source-end hook, module option body를 검사한다. 동적 이름은
  exact semantic 판정을 약속하지 않고 conservative may-depend 정책을 사용한다.
  따라서 persisted `\csname`, `\ifcsname`, `\@nameuse` 또는 그 primitive alias도
  capability를 요구하며 false-positive attachment suppression은 허용한다. 이는 unsafe
  legacy replay보다 fresh source rebuild를 선택하는 명시적 호환성 계약이다.
- Alias/scalar capability state는 legacy serializer에서 첫 byte 전에 거부되고,
  explicit legacy projection decode도 같은 구조 판정으로 fail-closed된다. Preamble,
  shipout, input-boundary attachment는 모두 none이며, compiler unchanged-tail reuse는
  필요한 모든 prior page checkpoint가 실제 restore 가능할 때만 선택된다. Suppressed
  state가 있는 다음 revision은 사용자 오류 대신 source rebuild로 동일 output을 만든다.
- Source implementation Pro review `6a7cb02d-855c-83ee-9ffe-36371addf309`은 초기
  candidate를 **REVISE**(confidence 약 0.90)로 판정했다. 지적된 dynamic-name
  laundering, lossy plus/minus, division overflow, failure-completion 누수는 각각
  RED/GREEN으로 닫았다. Mid-command capture 지적은 input-enter가 primitive token을
  requeue한 snapshot이고 input-exit가 dispatcher boundary라는 실제 구조로 기각했다.
  Primitive legacy meaning은 enum ordinal이 아니라 string name DTO이므로 variant 삽입에
  따른 wire discriminant 위험도 해당하지 않는다. 이 source-only 단계에서는
  versioned supported capability 집합과 durable writer를 계속 비활성으로 유지했다.
- `2cad7c2`는 `eqtb.muskip.alias-v1`과 `eqtb.muskip.scalar-v1` reader를 활성화한다.
  Document header의 format/schema/지원 capability를 state보다 먼저 검사하고, raw
  state를 strict duplicate-aware visitor로 읽은 뒤 exact `LegacyVmSnapshotV1`과 두
  muskip field를 결합한다. 선언 capability와 state-derived capability 집합은 정확히
  같아야 하므로 누락과 false claim을 모두 거부한다. Alias-only, scalar-only, 두
  capability 조합은 각 의미 상태에 맞게 복원되며 durable versioned writer와
  capability-bearing legacy writer는 계속 비활성이다.
- Capability-reader Pro review `6a7da491-0088-83ea-857d-c30bb49ea813`는 초기
  candidate를 **REVISE**(confidence 약 0.88)로 판정했다. `serde_json::Value`가
  duplicate state member를 leaf decoder 전에 소실시키는 blocker를 직접 document,
  nested checkpoint, production gzip+base64 envelope RED로 재현했다. Raw JSON은
  `StoredCheckpointWire` → `VersionedSnapshotSlotWire` → document state 전 경로에서
  보존하고, duplicate state/register member와 noncanonical register index를
  fail-closed로 거부하도록 수정했다. 기존 production save의 sentinel/temporary-file
  불변성과 cursor 255/`u32::MAX` allocator 비랩 회귀도 재확인했다.
- 확장된 exact `00c8ee3` matrix에서 현재 reader는 scalar envelope를 replay-safe
  hit로 복원해 `[2.5mu][3mu][3mu]`를 만들고, duplicate cursor envelope는 unreadable
  miss다. 같은 scalar envelope를 pre-reader binary가 읽으면 versioned field를
  보존하되 snapshot attachment가 없어 replay-safe가 아니며 source rebuild 경계로
  넘어간다. Legacy 양방향 호환, dual lane, future capability, malformed 문서 결과도
  유지된다. Reader 전체 gate는 tex-vm 663 unit+모든 integration, tex-checkpoint
  49+12+21, latexd lib 237, workspace test-target check, canonical Clippy/fmt가 green이다.
- `436e246`은 canonical versioned muskip document와 checkpoint attachment writer를
  구현하되 production 정책을 기존 public `SnapshotWritePolicy::LegacyOnly`에 고정한다.
  `VmSnapshotDocument::from_snapshot`과 custom serializer는 complete state에서
  capability header를 유도하고 format/schema/지원 capability/exact state equality를
  첫 byte 전에 검증한다. Scalar-only, alias-only, combined shape는 strict reader로
  round-trip하며 migration probe도 같은 실제 serializer output을 사용한다.
- Versioned lane 선택, bundle serializer, builder, save helper는 crate-private
  `SnapshotWriteMode`로만 접근할 수 있다. Public policy enum에는 versioned variant가
  없음을 compile-fail doctest로 고정했고, public builder/Serde/save는 모두 상수
  `LegacyOnly`를 주입한다. Capability-free state는 private full policy에서도 legacy
  lane에만 배치된다. 이미 versioned lane에 들어온 같은 state는 direct checkpoint,
  late-invalid-child bundle, private policy serializer, public atomic save에서 출력이나
  파일 변경 전에 거부된다. Invalid public slot도 nested document를 선검증한다.
- Disabled-writer Pro review
  `6a7db198-0c34-83e8-8630-f204d8985b8f`는 초기 evidence packet을 **REVISE**했다.
  가장 중요한 exact-lane 우려는 구현이 이미 `Some(Versioned)` equality를 사용했고
  기존 integration fixture도 empty-capability versioned lane을 거부했지만, direct,
  late-child, private full-policy, filesystem matrix를 추가해 명시적으로 닫았다. 공개
  enum의 새 variant는 실제 API/semver 위험이라 제거하고 private routing으로 옮겼다.
  Slot zero-byte preflight는 기존 custom serializer를 새 regression으로 확인했다.
  Reader/write semantic-validity closure, golden byte/insertion-order contract, 세 checkpoint
  category의 enabled-path restore, resource/telemetry는 activation 전 `ARCH-014` gate로
  남긴다.
- `436e246` 검증은 tex-vm 663 unit+모든 integration(document contract 23),
  tex-checkpoint 54+12+21+doctest, latexd lib 237, workspace test-target check, canonical
  Clippy/fmt, Python 8을 통과했다. Exact `00c8ee3` matrix에서 canonical raw muskip
  document는 pre-reader가 거부하고, candidate envelope는 `[2.5mu][3mu][3mu]`로
  replay-safe hit이며, pre-reader envelope는 attachment 없이 source rebuild 경계에
  남는다. Production이 유기적으로 생성하는 versioned checkpoint는 여전히 0이다.
- `059be28`은 이 0-artifact invariant를 production build artifact에서 관찰할 수
  있게 한다. Internal/external `build-meta.json`은 shared
  `SNAPSHOT_WRITE_POLICY`를 `checkpoint_writer_policy: "legacy_only"`로 기록하고,
  실제 완성 bundle의 none/legacy/versioned attachment 수를
  `checkpoint_attachment_counts`에 기록한다. Muskip state suppression은
  `none=N, legacy=0, versioned=0`, 정상 legacy state는 `legacy>0, versioned=0`으로
  고정된다. 두 field가 없는 이전 build metadata는 `legacy_only`와 zero counts로
  읽히므로 additive artifact compatibility를 유지한다. 이는 관측 gate이며 writer
  activation이나 외부 policy injection API가 아니다.
- Resource audit에서 envelope reader는 선언한 uncompressed length까지만 gzip stream을
  읽고 정확히 1 byte를 더 확인해 초과를 거부하며, 8 GiB ceiling도 해제 전에
  검사함을 재확인했다. Workspace의 남은 `arxiv-basic` checkpoint 표본은 최대 약
  1.17 MiB지만 모두 현재 compact envelope의 representative uncompressed corpus가
  아니므로 이를 근거로 production cap을 임의 축소하지 않는다. Writer가 자신이 만든
  artifact를 reader가 거부하지 않도록 현재 8 GiB ceiling은 측정과 무관하게 save에도
  먼저 대칭 적용한다. 더 낮은 production cap만 실제 long-paper uncompressed size/RSS
  측정 뒤 read/save에 함께 적용한다.
- `d1fc0c2`는 Pro review의 writer/read semantic-validity gap을 닫았다.
  `Vm::try_restore`의 context-independent root-scope, muskip cursor, primitive-name
  validation을 writer와 공유한다. 따라서 exact capability header를 가진 문서라도
  root scope가 없거나 cursor가 256 미만이거나 unknown primitive를 참조하면 canonical
  document serializer가 첫 byte 전에 실패하고, 이를 감싼 slot/checkpoint preflight도
  같은 판정을 사용한다. Reader의 기존 decode→fallible restore 두 단계와 error type은
  유지된다. Tex-vm 전체(document contract 24), tex-checkpoint 전체, latexd lib 237,
  workspace check/Clippy/fmt와 exact `00c8ee3` matrix가 green이다.
- `1a91712`는 같은 snapshot instance를 재사용한 기존 determinism test가 놓친
  `HashMap` seed/order 의존을 독립 VM RED로 재현했다. Canonical writer는 outer
  serializer를 열기 전에 exact legacy projection을 정렬된 `serde_json::Value`
  object로 정규화하고 두 muskip field를 충돌 검사 후 삽입한다. 따라서 의미상 같은
  독립 snapshot은 byte-identical하며, future legacy DTO가 reserved muskip key를
  추가하면 duplicate output 대신 zero-byte error가 난다. Scalar-only, alias-only,
  combined compact JSON의 exact length+Blake3 golden을 고정했고 서로 다른 test
  process에서도 document contract 25가 같은 digest로 통과한다. 여기서 canonical은
  현재 지원 `serde_json` compact encoding의 고정 bytes를 뜻하며 임의 serializer나
  RFC canonical JSON 일반 계약을 뜻하지 않는다. Full tex-vm/checkpoint/latexd,
  workspace check/Clippy/fmt와 `00c8ee3` matrix도 green이다.
- `c92f53a`는 production builder가 checkpoint wire와 별개인 typed
  `CheckpointBundleBuild`를 반환하게 하고, 실제 capture candidate가 탈락한 이유를
  `unsafe_continuation`과 `unsupported_capabilities`로 집계한다. Preamble과 input
  boundary는 항상 후보이고, shipout snapshot이 애초에 없는 page는 suppression으로
  오계수하지 않는다. Internal compiler는 성공한 build의 이 counts를 additive
  `checkpoint_suppression_counts`로 `build-meta.json`에 기록한다. Muskip source
  rebuild 회귀는 모든 후보를 unsupported capability로, unsafe preamble 회귀는
  정확히 1건을 unsafe로, 정상 legacy build와 external build는 zero counts로 고정한다.
  이전 metadata는 missing field를 zero로 읽는다. 이 수치로 conservative dynamic-name
  suppression 비용을 측정할 수 있지만 write-time lane mismatch/invalid-document error
  telemetry까지 대체하지는 않는다. Tex-checkpoint 55+12+21+doctest, latexd lib 237,
  workspace check/Clippy/fmt와 exact `00c8ee3` matrix가 green이다.
- Follow-up Pro review `6a7dc38f-c90c-83e8-bcaf-991e258ab10f`는 현재 disabled
  versioned-muskip writer phase 종료를 **APPROVE**(confidence 0.84)했다. Public/default
  checkpoint builder/Serde/save 중 versioned lane을 허용하는 경로와 현재 phase를
  무효화하는 P0/P1 결함은 발견하지 못했다. 단, public
  `VmSnapshotDocument::from_snapshot`과 document `Serialize`로 raw versioned document
  bytes를 만드는 것은 의도된 document API이며, “비활성”은 checkpoint attachment
  emission lane에 한정한다. 결론은 **disabled writer core complete / production writer
  activation blocked**이다.
- 같은 review가 activation 전 repository-local P1을 category-complete enabled path,
  policy-independent semantic state hash contract, future policy 값을 보존하는 tolerant
  metadata observation, reader의 현재 8 GiB ceiling과 같은 save admission, typed
  write-failure telemetry, exact release feature/profile reproducibility로 분류했다. Reader
  fleet/rollback floor와 source-rebuild 신뢰성, representative size/RSS, production
  filesystem semantics는 저장소 green만으로 닫을 수 없는 외부 evidence다.
- `0723906`은 첫 P1인 category coverage를 닫았다. Legacy preamble과 muskip-bearing
  shipout/input-boundary를 한 private full-policy bundle에 넣고, 두 category가 정확히
  versioned lane을 택하며 metadata를 보존한 채 compact envelope save/read,
  `Vm::try_restore`, `[2.5mu][category]` replay까지 통과함을 고정한다. Partial capability와
  unsafe continuation은 두 category에서 각각 `unsupported_capabilities=2`와
  `unsafe_continuation=2`로 suppress되고 attachment는 none이다. Bundle-owned
  `CheckpointAttachmentCounts`가 test와 compiler `build-meta.json`의 실제 lane 집계를
  함께 소유한다. `VmRestoreError`의 현재 세 variant도 모두 shared preflight가 판정하며
  `try_restore`에는 그 뒤 fallible tail이 없음을 재감사했다. Tex-checkpoint
  58+12+21+doctest, latexd lib 237, workspace check, canonical Clippy/fmt가 green이다.
- `5a7bd82`는 두 번째 P1인 semantic state-hash contract를 실제 RED로 닫았다.
  기존 legacy branch와 non-legacy fingerprint의 legacy projection은 nested
  `HashMap` iteration order를 그대로 hash해 독립 생성한 동등 snapshot이 서로 다른
  값을 냈다. 이제 두 branch 모두 정렬된 `serde_json::Value` projection을 입력으로
  사용하고 함수 이름과 주석도 policy-independent semantic identity를 명시한다.
  Legacy/muskip golden, 독립 VM equality, scalar/alias/cursor/legacy-field mutation,
  production suppression/private versioned routing equality가 이를 고정한다. 소비자는
  `CheckpointMeta.vm_state_hash`와 같은 bundle의 `checkpoint_id` 생성뿐이고 reader가
  재계산 비교하지 않으므로 새 capture ID는 canonical 값으로 한 번 전환되지만 기존
  bundle read/replay contract는 유지된다. Tex-checkpoint 59+12+21+doctest, latexd lib
  237, workspace check, canonical Clippy/fmt와 exact `00c8ee3` matrix가 green이다.
- `3c6798b`는 세 번째 P1인 writer-policy metadata forward tolerance를 닫았다.
  `BuildMeta.checkpoint_writer_policy`가 strict route-control enum을 직접 읽어 미래
  `"future_versioned_muskip"` 값에서 전체 metadata decode를 실패시키는 RED를 추가했다.
  새 `SnapshotWritePolicyObservation`은 `legacy_only`를 typed value로, 그 밖의 문자열을
  `Other(String)`으로 손실 없이 보존·재직렬화한다. Builder/save는 이 observation을
  받지 않고 기존 단일 public `SnapshotWritePolicy::LegacyOnly`와 crate-private mode만
  사용하므로 tolerant reader가 activation surface를 다시 만들지 않는다. Missing field
  default와 internal/external metadata의 기존 `"legacy_only"` wire도 유지된다.
  Tex-checkpoint 59+12+21+doctest, latexd lib 237, workspace check, canonical Clippy/fmt가
  green이다.
- `499db9a`는 네 번째 P1인 reader/writer resource admission 대칭을 닫았다.
  Public save와 envelope load가 같은 8 GiB uncompressed ceiling을 주입하고, streaming
  `IntegrityWriter`가 limit을 넘기는 write를 typed
  `CheckpointUncompressedSizeLimitExceeded { attempted, limit }`로 중단한다. Test-only
  작은 limit에서 exact byte save/read는 성공하고 +1 byte save는 persist 전에 실패하며
  기존 sentinel target과 directory entry 수가 유지되어 temporary file cleanup도
  확인된다. 이 correctness ceiling은 이제 닫혔고, representative long-paper size/RSS를
  근거로 더 낮은 운영 상한을 정하는 일만 rollout evidence로 남는다. Tex-checkpoint
  60+12+21+doctest, latexd lib 237, workspace check, canonical Clippy/fmt와 exact
  `00c8ee3` matrix가 green이다.
- `2469a35`는 다섯 번째 P1인 typed write outcome telemetry를 닫았다.
  Checkpoint save는 low-cardinality `lane_mismatch`, `invalid_document`,
  `bundle_preflight`, `size_limit`, `serialization`, `integrity_envelope`, `tempfile`,
  `persist` failure reason과 성공 시 uncompressed/persisted byte 수를 반환한다.
  Internal compiler는 attempt마다 `checkpoint-write-outcome.json`을 쓰므로 save가
  실패해 후속 `build-meta.json`이 없더라도 typed reason을 수집할 수 있다. 성공한
  outcome은 additive `checkpoint_write_outcome` metadata에도 같은 값으로 들어가고,
  external compiler는 `not_attempted`, 이전 metadata의 missing field도 `not_attempted`로
  읽힌다. Lane mismatch/invalid document/size/tempfile/persist와 성공 artifact를
  executable tests로 고정했으며 capture suppression은 write attempt와 계속 분리된다.
  Tex-checkpoint 61+12+21+doctest, latexd lib 238, workspace check, canonical Clippy/fmt와
  exact `00c8ee3` matrix가 green이다.
- `e4d54f5`는 여섯 번째이자 마지막 repository-local P1인 exact native release
  feature/profile gate를 닫았다. 저장소에 별도 production packaging/deployment profile이
  없었으므로 canonical native contract를 Rust `1.94.0`,
  `x86_64-unknown-linux-gnu`, Cargo default features, `--release --locked`로 명시했다.
  `scripts/check_snapshot_release_contract.py`는 toolchain/host와 `Cargo.lock`을 확인하고,
  production binary인 `latexd`에서 해석된 전체 feature graph 및 그 hash, lock hash,
  rustc 상세 버전, revision, 실제 명령을 JSON artifact로 기록한다. Serializer에 영향을
  주는 `serde_json` feature는 정확히 `default`, `raw_value`, `std`여야 하며 다른 값은
  fail-closed다. 서로 다른 두 clean target directory의 25개 fixed compact-document
  golden, release `tex-checkpoint` 61+12+21+doctest, real internal compiler의 policy/count 및
  muskip suppression→source rebuild 회귀, release/target-pinned exact `00c8ee3`
  bidirectional matrix가 모두 green이다. CI도 같은 toolchain/target을 pin하고 결과를
  `snapshot-release-contract` artifact로 보존한다. Python policy suite 31개도 green이다.

여섯 repository-local P1은 모두 닫혔다. 다음 순서는 실제 배포 artifact가 위 canonical
contract와 일치하는지 확인하고, reader fleet 선배포/rollback floor,
source-rebuild reliability, representative size/RSS, production filesystem semantics를
수집하는 외부 rollout evidence다. 이 evidence와 별도 activation review 없이는 writer를
활성화하지 않는다. 현재 source slice의
conservative dynamic-name suppression 빈도는 activation 전에 측정하고, 필요할 때만
binding-aware precision을 후속 최적화한다. 어느 단계에서도
capability-bearing state를 legacy lane에 쓰지 않으며, non-legacy state는 save 오류가
아니라 attachment suppression과 정상 source rebuild로 귀결한다.

진입 gate:
- production diff는 file/source revision, expansion,
  `ExecutedSourceSlice`, source path/span, compiler build revision을 참조하지
  않고 event `sequence`를 identity로 해석하지 않는다.
- 기존 interner-local `ControlSequenceId` lifetime과 현재 event schema v6의
  v5 reader compatibility, HMR/WASM wire, checkpoint format version을 유지한다.
- snapshot representation은 deterministic byte equality 또는 동일 version의
  decode equality를 유지하고, nested local/global assignment, `\globaldefs`,
  group unwind, snapshot/restore, continuation/replay의 Eqtb/SaveStack state,
  events, diagnostics, recovery-visible behavior가 현재 `d9cdf02` baseline과
  같다.
- 첫 batch는 production owner를 옮기지 않고 위 differential fixture와
  changed-path/added-symbol guard를 추가했다 (`2289907`, `fe6b4df`,
  `d9cdf02`). 다음 batch가 same-name global semantics를 바로잡고 공통 storage를
  추가한 뒤 owner를 이전했다 (`f66cdbf`, `c640efb`, `775cc22`). 이후 Pro
  리뷰 remediation은 projection validation, legacy open-group compatibility,
  boxed value layout, lexical front-matter policy, exhaustive oracle, malformed
  snapshot boundary와 fallible production restore를 각각 독립 rollback 단위로
  닫았다. Exact layered mixed-version matrix와 clean-commit 재검증 뒤 Pro closure
  gate도 green이다. Persisted field, command-ID lifetime, provenance,
  event/replay output은 변경하지 않았다.

이전 순서:
1. control sequence definition과 `\let` — 완료 (`775cc22`)
2. count와 arithmetic
3. dimen
4. skip — 완료; muskip — readers-first document와 dual checkpoint reader,
   legacy eligibility/suppression seam, typed runtime owner, complete in-memory
   snapshot/capability, strict legacy boundary, source allocator/alias/scalar assignment,
   arithmetic, dynamic-name capability, fresh-source rebuild, strict capability reader,
   private-policy disabled writer 완료. Writer activation은 별도 fleet/observability
   gate로 남아 있다 (`e3bec73`, `dcbee7c`, `1d29aaa`, `8d91fd3`, `a2466c7`,
   `b809c30`, `f899127`, `f02a4cd`, `6604eb7`, `2cad7c2`, `436e246`).
5. toks
6. catcode
7. mathcode/delcode
8. font/box/parameter

Mathcode/delcode 진입 계획은 Pro review
`6a7ddf11-4714-83ea-a87c-80da662df53f`의 `REVISE` 결론을 반영한다. 두
family는 assignment/SaveStack 기반은 공유하지만 capability, document state,
source activation, rollback은 분리한다. 순서는 oracle characterization → passive
strict reader → snapshot-safe dormant Eqtb → `mathcode` source activation → `delcode`
source activation이다. Source에서 도달 가능한 assignment는 current/pending state와
latent primitive owner를 snapshot이 완전하게 보존하고 raw legacy serialization 및
LegacyOnly checkpoint attachment가 fail-closed/suppress되는 단위와 동시에만 들어간다.
현재 source-normalizing math renderer는 code table을 소비하지 않으므로 이 단계는
storage/query/persistence compatibility substrate이며 math spacing, family/slot glyph,
active-mathcode 실행, delimiter selection, `\mathchar`, `\mathchardef` 실행을 포함하지
않는다.

`328859a`는 첫 characterization gate를 추가했고, 그 후속 hardening은 authoritative
target을 `pdfTeX -ini`의 TeX82 semantics로 고정한다. CI artifact
`mathcode-delcode-oracle`은 실행 binary의 전체 version, resolved path와 SHA-256,
probe source/hash, exit status, normalized observations/diagnostics를 보존한다. V1 source
character domain은 `0..=255`이며 invalid LHS는 TeX recovery에 따라 character 0으로
대체된 뒤 assignment가 계속된다. Fresh INITEX mathcode default는 digit에
`0x7000 + character`, ASCII letter에 `0x7100 + character`, 나머지 8-bit character에
character 자체를 사용한다. Delcode default는 `'.'`만 0이고 나머지는 -1이다.
Mathcode RHS는 `0..=32768`이고 `32768`은 active sentinel이며, 범위 밖 RHS는 진단 뒤
0을 저장한다. Delcode RHS는 `-2147483647..=16777215`를 그대로 보존하고 상한 초과는
진단 뒤 0을 저장한다. Decimal/octal/hex/backtick character scanning, optional `=`,
`\the`와 `\number`, local/global/nested/positive·negative `\globaldefs`, 256-entry full
default table을 oracle이 검사한다. `\mathchardef`는 32767까지이고 32768은 진단 뒤
0-valued meaning을 만들며, 별도 control-sequence meaning migration으로 남긴다.

Control-sequence slice closeout 뒤 남은 non-blocking follow-up은 malformed
restore의 non-empty interner와 deep-layer atomicity test, generated public JSON
project/restore/project property, restore time/RSS 측정 뒤 별도 versioned
resource-limit 결정이다. Production `Vm::restore` 정적 guard는 완료됐다. 남은
항목들은 M13.3의 다음 assignment-class migration과 독립적으로 수행할 수 있다.

이전된 assignment는 공통 Eqtb `assign()` 경로에서 local 이전 값을
SaveStack에 한 번 저장하고 global assignment 시 pending restore를 취소한다.
Control-sequence module-base 암묵적 global 규칙은 CS scope resolver에만
남기며 register/catcode에는 적용하지 않는다.

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
   실제 producer/consumer별로 분류 — 전체 112개 inventory 완료
5. production family typed migration과 layered static guard는 완료
   (`776d604`, `75a79d5`). incidental fixture 이전 뒤 두 public raw constructor와
   24개 contract call을 함께 제거했고, 구조 guard와 legacy JSON read fixture를
   보강했다 (`247a647`, `80ca0e2`, `0940368`); serialized `Command`, lossy
   `Fallback`/low 의미는 별도 audit 전 유지
6. 7개 family의 location-only overlap 공통화와 네 family insertion anchor의
   origin metadata 분리는 `decccd7`, `694a0ee`에 landed. bibliography anchor도
   source geometry로 분리됐고 (`4c24516`), footnote sequence identity audit/fix도
   닫혔다 (`ba9424d`). M13.2의 남은 all-family suppression closeout을 완료
7. control-sequence behavior characterization — 기존 grouping/global/
   `\globaldefs`/snapshot tests green
8. 기존 layered scope representation을 semantics 변경 없이 bounded
   `ControlSequenceScopes` module/API로 격리 — `94d277e` landed
9. production owner 이전 전 M13.3 independence differential/compatibility/
   changed-path/added-symbol guard 추가 — `2289907`, `fe6b4df`, `d9cdf02` landed
10. control-sequence definition/`\let` 한 owner를 기존 serialized snapshot
    shape와 replay behavior를 유지해 Eqtb/SaveStack으로 이전 — `f66cdbf`,
    `c640efb`, `775cc22` landed
11. remaining assignment class를 이전하고 persistent root/state hash 구현
12. M13.4 identity ADR 승인 뒤 source registry shadow mode, lexical origin,
    expansion/scoped command identity, readers-first snapshot capability,
    validated internal `ExecutedSourceSlice` 순서로 구현
13. streaming Mouth와 macro/command boundary를 구현한 뒤 EngineState와
    execution mode/nest 통합
14. M13.6 family별 bounded recovery/SemanticSink migration; scanner state parity,
    legacy snapshot policy, zero divergence/fallback 뒤 최종 retirement
15. Snapshot v2와 transactional replay
16. SemanticDocumentIr metadata/frame builder와 LayoutIr
17. MathList/수식 layout/AMS/OpenType 순서

Public event identity/schema와 shared diagnostic contract는 위 critical path에
끼워 넣지 않고 각각 별도 Pro/schema review와 readers-first adapter migration으로
진행한다.

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
