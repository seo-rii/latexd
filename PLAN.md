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

- 기준 시점: `2026-07-26`
- `M11`: `M11.1`~`M11.4` 완료
- `M12`: `M12.1`~`M12.6` 완료
- 현재 집중 범위: `M13` 단일 VM 실행 의미와 실행 기반 event/IR/checkpoint

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
- `lib.rs`에는 여전히 source recovery와 compatibility surface가 크게 남아
  있으므로 mechanical split 완료로 보지는 않는다.

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
- scanner event는 `ScannerRecovery`와 medium confidence를 명시한다.
- migrated family는 primitive/macro 실행 event로 scanner 결과를
  reconciliation한다.
- sequence/stable identity 분리와 bounded recovery 전환은 남아 있다.

- `Primitive`, `Macro`, `CompatCommand`, `Shim`, `BblParser`,
  `ScannerRecovery`, `Fallback`, `Unknown` producer를 명시한다.
- constructor가 producer/confidence를 필수로 받게 한다.
- 현재 `event_id`를 build-local `sequence`로 정직하게 versioning한다.
- whole-source scanner는 미이전 event family의 명시적 low-confidence
  compatibility bridge와 debug differential로만 동결하고 새 기능을
  추가하지 않는다.
- M13.6에서 family별로 bridge를 제거하며, 최종 복구 scanner는 VM이
  실제 실행한 bounded source slice에만 호출한다.
- mouth/expansion/execution/lowering/checkpoint/layout/rendering이 공유하는
  code/severity/provenance/recovery/phase diagnostic schema를 추가한다.

완료 조건:
- false conditional leakage는 해당 family 이전 전까지 known failing
  characterization으로 드러나며 high-confidence output으로 숨기지 않는다.
- scanner recovery event는 medium/low confidence로 식별된다.
- sequence를 revision-stable identity로 사용하지 않는다.
- `StableEventId`는 M13.4의 token/expansion origin 이전에는 만들지 않는다.

### M13.3 Eqtb And SaveStack

상태: `in progress`

현재 구현:
- control-sequence definition과 주요 register/catcode assignment가 공통
  Eqtb/SaveStack scope 경로를 사용한다.
- 남은 assignment class 이전, old scope 제거, persistent root/state hash는
  완료 전이다.

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

1. VM semantic divergence characterization
2. unsafe continuation checkpoint reuse 보수 차단
3. behavior-preserving `tex-vm` module split
4. scanner quarantine와 explicit event origin
5. phase-aware structured diagnostic contract
6. Eqtb/SaveStack definition+count slice
7. dimen/skip/toks/catcode/mathcode/font assignment migration
8. streaming Mouth와 TokenOrigin
9. macro parameter text/prefix/command split
10. EngineState와 execution mode/nest 통합
11. token/expansion 기반 stable event ID와 SemanticSink event family migration
12. Snapshot v2와 transactional replay
13. SemanticDocumentIr metadata/frame builder와 LayoutIr
14. MathList/수식 layout/AMS/OpenType 순서

브라우저 PDF/pages와 font resolver는 disjoint file lane에서 병렬 진행할 수
있다. persistent session은 Snapshot v2보다 먼저 시작하지 않는다.

## CI Gates

### Default

- VM module unit/characterization tests
- local/global assignment와 catcode/input provenance
- macro parameter/prefix expansion
- event producer/confidence/sequence goldens과 origin 도입 뒤 stable-ID goldens
- event-to-IR mapping과 builder recovery
- format/continuation snapshot equivalence
- browser PDF/pages artifact
- compact font/native-WASI parity와 math atlas

### Push

- broader VM/IR/checkpoint/internal compiler tests
- conditional/macro/package interaction
- selected AMS/alignment/package fixtures
- external-engine differential artifact
- browser screenshot/source-span checks

### Nightly/Manual

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
