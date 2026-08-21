use tex_tfm_metrics::{TfmParseError, TfmScaleError, parse_tfm};

const CMR10: &[u8] = include_bytes!("../../tex-fonts/assets/classic/tfm/cmr10.tfm");
const CMR7: &[u8] = include_bytes!("../../tex-fonts/assets/classic/tfm/cmr7.tfm");

#[test]
fn computer_modern_dimension_metrics_match_the_native_oracle_exactly() {
    let cmr10 = parse_tfm(CMR10).expect("cmr10 must be a valid TFM");
    assert_eq!(cmr10.design_size_sp(), 10 * 65_536);
    assert_eq!(
        cmr10.at_size_sp(cmr10.design_size_sp()).unwrap().quad_sp,
        655_361
    );
    assert_eq!(
        cmr10
            .at_size_sp(cmr10.design_size_sp())
            .unwrap()
            .x_height_sp,
        282_168
    );
    assert_eq!(cmr10.at_size_sp(12 * 65_536).unwrap().quad_sp, 786_434);
    assert_eq!(cmr10.at_size_sp(12 * 65_536).unwrap().x_height_sp, 338_602);

    let cmr7 = parse_tfm(CMR7).expect("cmr7 must be a valid TFM");
    assert_eq!(cmr7.design_size_sp(), 7 * 65_536);
    assert_eq!(
        cmr7.at_size_sp(cmr7.design_size_sp()).unwrap().quad_sp,
        522_469
    );
    assert_eq!(
        cmr7.at_size_sp(cmr7.design_size_sp()).unwrap().x_height_sp,
        197_518
    );
}

#[test]
fn content_identity_is_tfm_only_and_byte_stable() {
    let first = parse_tfm(CMR10).unwrap();
    let second = parse_tfm(CMR10).unwrap();
    let distinct = parse_tfm(CMR7).unwrap();

    assert_eq!(first.content_hash(), second.content_hash());
    assert_ne!(first.content_hash(), distinct.content_hash());
    assert_eq!(
        first.content_hash(),
        "sha256:87f2d8981927644cbecaf3d639e96e348ea4e7be49d8804468bd8ba9ff3f5244"
    );
    assert_eq!(
        distinct.content_hash(),
        "sha256:28f9f4d237bdae4babef4a59d90f17686a09d6f9df0b2c3c23b2ba15f20eee82"
    );
}

#[test]
fn malformed_or_incomplete_tfm_data_fails_without_a_fallback() {
    assert_eq!(parse_tfm(&[0; 23]), Err(TfmParseError::TooShort));

    let mut bad_length = CMR10.to_vec();
    bad_length[0..2].copy_from_slice(&1u16.to_be_bytes());
    assert!(matches!(
        parse_tfm(&bad_length),
        Err(TfmParseError::LengthMismatch { .. })
    ));

    let mut missing_quad = CMR10[..CMR10.len() - 8].to_vec();
    missing_quad[0..2].copy_from_slice(&322u16.to_be_bytes());
    missing_quad[22..24].copy_from_slice(&5u16.to_be_bytes());
    assert_eq!(
        parse_tfm(&missing_quad),
        Err(TfmParseError::MissingFontDimension {
            required: 6,
            available: 5,
        })
    );

    let mut zero_design_size = CMR10.to_vec();
    zero_design_size[28..32].fill(0);
    assert_eq!(
        parse_tfm(&zero_design_size),
        Err(TfmParseError::InvalidDesignSize)
    );
}

#[test]
fn invalid_or_overflowing_effective_sizes_are_typed_errors() {
    let metrics = parse_tfm(CMR10).unwrap();

    assert_eq!(metrics.at_size_sp(0), Err(TfmScaleError::NonPositiveSize));
    assert_eq!(metrics.at_size_sp(i32::MAX), Err(TfmScaleError::Overflow));
}

#[test]
fn scaling_matches_tex82_at_negative_and_large_size_boundaries() {
    let cmr10 = parse_tfm(CMR10).unwrap();
    assert_eq!(cmr10.at_size_sp((1 << 23) + 1).unwrap().quad_sp, 8_388_632);

    let mut negative_x_height = CMR10.to_vec();
    let counts = (0..12)
        .map(|index| u16::from_be_bytes([CMR10[index * 2], CMR10[index * 2 + 1]]) as usize)
        .collect::<Vec<_>>();
    let [_, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, _] = counts.as_slice() else {
        unreachable!()
    };
    let character_count = ec - bc + 1;
    let parameter_start = 4 * (6 + lh + character_count + nw + nh + nd + ni + nl + nk + ne);
    negative_x_height[parameter_start + 16..parameter_start + 20]
        .copy_from_slice(&(-1i32).to_be_bytes());

    let metrics = parse_tfm(&negative_x_height).unwrap();
    assert_eq!(metrics.at_size_sp(1).unwrap().x_height_sp, -1);
}
