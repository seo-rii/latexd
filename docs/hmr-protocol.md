# Viewer HMR Protocol

This document describes the runtime message surface between the `latexd` backend and the
browser viewer.

## Message Surface

서버 메시지는 이 정도면 충분하다.

```rust
enum ServerMsg {
    BuildStarted {
        rev: RevId,
        changed_files: Vec<String>,
    },
    Diagnostics {
        rev: RevId,
        items: Vec<Diagnostic>,
    },
    FullPdfReady {
        rev: RevId,
        pdf_url: String,
    },
    PatchPages {
        rev: RevId,
        ops: Vec<PagePatchOp>,
    },
    PatchTiles {
        rev: RevId,
        page_id: PageId,
        tiles: Vec<TilePatch>,
    },
    SyncMapReady {
        rev: RevId,
        page_id: PageId,
        map_url: String,
    },
    BuildFinished {
        rev: RevId,
        success: bool,
    },
}
```

클라이언트 메시지:

```rust
enum ClientMsg {
    OpenDocument { doc: String },
    ViewportChanged {
        zoom: f32,
        visible_pages: Vec<PageId>,
        visible_tiles: Vec<TileRequest>,
    },
    JumpToSource { page_id: PageId, x: f32, y: f32 },
    RevealSpan { file: String, start: u32, end: u32 },
}
```

## Reducer Contract

reducer를 꼭 순수 함수로 만든다.

```ts
function reduce(state: ViewerState, msg: ServerMsg): ViewerState
```

이 reducer는 TS에서 가장 강하게 TDD를 적용할 부분이다.

---
