use tex_tfm_metrics::dimension_subset::{ExactTfmDimensions, ExtractError, extract_exact_frame};

const CMR10: &[u8] = include_bytes!("../../tex-fonts/assets/classic/tfm/cmr10.tfm");

#[test]
fn public_api_names_the_exact_frame_dimension_subset_and_removes_broad_aliases() {
    let source = include_str!("../src/lib.rs");

    assert!(source.contains("pub mod dimension_subset"));
    assert!(source.contains("pub fn extract_exact_frame"));
    assert!(source.contains("Success does not imply"));
    assert!(!source.contains("pub fn parse_tfm"));
    assert!(!source.contains("pub enum TfmParseError"));
}

#[test]
fn exact_frame_policy_rejects_native_accepted_trailing_data_without_claiming_invalidity() {
    let mut trailing_word = CMR10.to_vec();
    trailing_word.extend_from_slice(&[0; 4]);

    assert!(matches!(
        extract_exact_frame(&trailing_word),
        Err(ExtractError::ExactFrameLengthMismatch { .. })
    ));
}

#[test]
fn unrelated_native_invalid_table_data_can_still_expose_the_dimension_subset() {
    let control = extract_exact_frame(CMR10).unwrap();
    let mut invalid_fontdimen2 = CMR10.to_vec();
    let parameter_start = parameter_start(CMR10);
    invalid_fontdimen2[parameter_start + 4..parameter_start + 8]
        .copy_from_slice(&(1i32 << 24).to_be_bytes());

    let subset = extract_exact_frame(&invalid_fontdimen2)
        .expect("unselected fontdimen validity is outside this subset contract");
    assert_eq!(subset.design_size_sp(), control.design_size_sp());
    assert_eq!(
        subset.at_size_sp(subset.design_size_sp()).unwrap(),
        control.at_size_sp(control.design_size_sp()).unwrap()
    );
}

#[test]
fn empty_parameter_table_zero_fills_both_selected_dimensions() {
    let mut missing_parameters = CMR10[..CMR10.len() - 28].to_vec();
    missing_parameters[0..2].copy_from_slice(&317u16.to_be_bytes());
    missing_parameters[22..24].copy_from_slice(&0u16.to_be_bytes());

    let metrics = extract_exact_frame(&missing_parameters)
        .expect("np=0 exposes a valid zero-filled dimension subset");
    assert_eq!(
        metrics.at_size_sp(metrics.design_size_sp()).unwrap(),
        ExactTfmDimensions {
            quad_sp: 0,
            x_height_sp: 0,
        }
    );
}

fn parameter_start(bytes: &[u8]) -> usize {
    let counts = (0..12)
        .map(|index| u16::from_be_bytes([bytes[index * 2], bytes[index * 2 + 1]]) as usize)
        .collect::<Vec<_>>();
    let [_, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, _] = counts.as_slice() else {
        unreachable!()
    };
    let character_count = ec - bc + 1;
    4 * (6 + lh + character_count + nw + nh + nd + ni + nl + nk + ne)
}
