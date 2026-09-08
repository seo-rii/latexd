use crate::compiler::{CompileRequest, CompilerDriver};
use camino::Utf8PathBuf;
use tex_render_model::{DrawOp, PageDisplayList};

#[tokio::test]
async fn aux_page_numbers_follow_display_math_shipout_and_clean_rebuild() {
    let temp = tempfile::tempdir().unwrap();
    let root = Utf8PathBuf::from_path_buf(temp.path().to_owned()).unwrap();
    let build_root = root.join(".latexd/build");
    let mut source =
        String::from(r"\documentclass{article}\begin{document}See page \pageref{tail}.");
    source.push_str(&r"\[\frac{x_1+x_2}{y_1+y_2}=z^2\]".repeat(32));
    source.push_str(r"\section{Tail}\label{tail}Last paragraph.\end{document}");
    std::fs::write(root.join("main.tex"), &source).unwrap();
    let driver = CompilerDriver::new(Some("internal".into()), Vec::new());
    let mut observed = Vec::new();
    for (rev, root_out) in [
        (1, build_root.clone()),
        (2, build_root.clone()),
        (1, root.join("clean-build")),
    ] {
        let outcome = driver
            .compile(CompileRequest {
                root: root.clone(),
                manifest: tex_world::ProjectManifest::discover(&root).unwrap(),
                toplevel: "main.tex".into(),
                rev,
                build_root: root_out.clone(),
                changed_files: vec!["main.tex".into()],
            })
            .await
            .unwrap();
        let pages: Vec<PageDisplayList> = serde_json::from_slice(
            &std::fs::read(root_out.join(format!("rev-{rev}/render-ir/page-display-list.json")))
                .unwrap(),
        )
        .unwrap();
        let aux: tex_aux::SemanticAux = serde_json::from_slice(
            &std::fs::read(root_out.join(format!("rev-{rev}/aux.json"))).unwrap(),
        )
        .unwrap();
        let label = aux.labels.iter().find(|label| label.key == "tail").unwrap();
        let actual_page = pages.iter().position(|page| page.ops.iter().any(|op| matches!(op, DrawOp::NamedDestination(destination) if destination.name == "tail"))).unwrap_or_else(|| {
            let document: serde_json::Value = serde_json::from_slice(&std::fs::read(root_out.join(format!("rev-{rev}/render-ir/document-ir.json"))).unwrap()).unwrap();
            panic!("missing tail destination; IR labels={}, page count={}, aux labels={:?}", document["labels"], pages.len(), aux.labels);
        }) as u32 + 1;
        assert!(actual_page > 1);
        assert_eq!(label.page, actual_page);
        assert_ne!(
            outcome.page_metadata.len(),
            pages.len(),
            "fixture must distinguish legacy text and presentation pagination"
        );
        observed.push((aux, pages));
    }
    assert_eq!(observed[0], observed[1]);
    assert_eq!(observed[1], observed[2]);
}
