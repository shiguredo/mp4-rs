//! `src/boxes_cenc.rs` に定義される CENC 関連ボックスの Property-Based Testing
//!
//! `PsshBox` / `FrmaBox` / `SchmBox` / `SchiBox` / `SinfBox` / `TencBox` について、
//! ランダムに生成した値の encode / decode 往復を検証する。

use noprop::TestCaseContext;
use shiguredo_mp4::{
    BoxSize, BoxType, Decode, Encode, Uint, Utf8String,
    boxes::{FrmaBox, PsshBox, SchiBox, SchmBox, SinfBox, TencBox, UnknownBox},
};

/// このファイルの主要 PBT ケース数（他のボックス系 PBT と揃える）
const CASES: usize = 500;

/// noprop の `sample_usize_in` で長さを引いてから要素を生成するベクタサンプラー
fn sample_vec<T>(
    ctx: &mut TestCaseContext,
    range: std::ops::Range<usize>,
    mut elem: impl FnMut(&mut TestCaseContext) -> T,
) -> Vec<T> {
    let len = noprop::sample_usize_in(ctx, range);
    let mut result = Vec::new();
    for _ in 0..len {
        result.push(elem(ctx));
    }
    result
}

/// 指定された長さの範囲で任意のバイト列を生成する
fn sample_bytes_vec_in(
    ctx: &mut TestCaseContext,
    range: std::ops::RangeInclusive<usize>,
) -> Vec<u8> {
    let len = noprop::sample_usize_in(ctx, range);
    noprop::sample_bytes_vec(ctx, len)
}

/// null 文字を含まない任意の UTF-8 文字列から `Utf8String` を生成する
///
/// `Utf8String` は null 文字を含む文字列を受け入れないため、null 文字を除外する
/// （`pbt/tests/prop_boxes.rs` の `arb_track_name` と同じ生成方針）
fn arb_utf8_string(ctx: &mut TestCaseContext) -> Utf8String {
    let len = noprop::sample_usize_in(ctx, 0..=32);
    let mut s = String::new();
    while s.chars().count() < len {
        let c = noprop::sample_char(ctx);
        if c != '\0' {
            s.push(c);
        }
    }
    Utf8String::new(&s).expect("null 文字を含まない")
}

/// 16 バイトの KID を生成する
fn arb_kid(ctx: &mut TestCaseContext) -> [u8; 16] {
    noprop::sample_bytes::<16>(ctx)
}

// ===== サンプラー定義 =====

/// PsshBox (version 0) を生成する
///
/// version 0 は KID の一覧を持てないため、`kids` は必ず空にする。
fn arb_pssh_box_v0(ctx: &mut TestCaseContext) -> PsshBox {
    PsshBox {
        version: 0,
        system_id: noprop::sample_bytes::<16>(ctx),
        kids: Vec::new(),
        data: sample_bytes_vec_in(ctx, 0..=64),
    }
}

/// PsshBox (version 1) を生成する
///
/// version 1 は KID 0 件（ムービー内の全 KID に適用する表現）も有効なので、
/// 0 件を含めた件数で生成する。
fn arb_pssh_box_v1(ctx: &mut TestCaseContext) -> PsshBox {
    PsshBox {
        version: 1,
        system_id: noprop::sample_bytes::<16>(ctx),
        kids: sample_vec(ctx, 0..5, arb_kid),
        data: sample_bytes_vec_in(ctx, 0..=64),
    }
}

/// FrmaBox を生成する
fn arb_frma_box(ctx: &mut TestCaseContext) -> FrmaBox {
    FrmaBox {
        data_format: noprop::sample_bytes::<4>(ctx),
    }
}

/// SchmBox を生成する
///
/// `scheme_uri` は flags の `0x000001` が立っている場合だけ存在するため、
/// あり・なしの両方を生成する。
fn arb_schm_box(ctx: &mut TestCaseContext) -> SchmBox {
    SchmBox {
        scheme_type: noprop::sample_bytes::<4>(ctx),
        scheme_version: noprop::sample_u32(ctx),
        scheme_uri: if noprop::sample_bool(ctx) {
            Some(arb_utf8_string(ctx))
        } else {
            None
        },
    }
}

/// TencBox を生成する
///
/// version 0（パターン暗号化パラメーターなし）と version 1（あり）、
/// constant IV あり・なしの組み合わせを生成する。
/// constant IV は `default_is_protected == 1 && default_per_sample_iv_size == 0` の場合だけ
/// 持てるため、その条件を満たすときだけ付与する。
fn arb_tenc_box(ctx: &mut TestCaseContext) -> TencBox {
    let default_is_protected: u8 = noprop::sample_u64_in(ctx, 0..=1) as u8;
    let use_constant_iv = default_is_protected == 1 && noprop::sample_bool(ctx);
    let (default_per_sample_iv_size, default_constant_iv): (u8, Option<Vec<u8>>) =
        if use_constant_iv {
            // constant IV は 8 バイトまたは 16 バイトでなければならない
            let iv_len = if noprop::sample_bool(ctx) { 8 } else { 16 };
            (0, Some(noprop::sample_bytes_vec(ctx, iv_len)))
        } else {
            // per-sample IV は 8 バイトまたは 16 バイトでなければならない
            let iv_size = if noprop::sample_bool(ctx) { 8 } else { 16 };
            (iv_size, None)
        };
    let (default_crypt_byte_block, default_skip_byte_block) = if noprop::sample_bool(ctx) {
        // パターン暗号化パラメーターは 4 ビットに収まる値でなければならない
        (
            Some(Uint::new(noprop::sample_u64_in(ctx, 0..=0x0F) as u8)),
            Some(Uint::new(noprop::sample_u64_in(ctx, 0..=0x0F) as u8)),
        )
    } else {
        (None, None)
    };
    TencBox {
        default_is_protected,
        default_per_sample_iv_size,
        default_kid: arb_kid(ctx),
        default_constant_iv,
        default_crypt_byte_block,
        default_skip_byte_block,
    }
}

/// UnknownBox を生成する
fn arb_unknown_box(ctx: &mut TestCaseContext) -> UnknownBox {
    let box_type = noprop::sample_bytes::<4>(ctx);
    let payload = sample_bytes_vec_in(ctx, 0..=32);
    UnknownBox {
        box_type: BoxType::Normal(box_type),
        box_size: BoxSize::with_payload_size(BoxType::Normal(box_type), payload.len() as u64)
            .expect("テスト用のペイロード長は総サイズが u64 に収まる"),
        payload,
    }
}

/// SchiBox を生成する
///
/// CENC では `tenc` が必須なので常に持たせ、未知の子ボックスも一緒に生成する。
fn arb_schi_box(ctx: &mut TestCaseContext) -> SchiBox {
    SchiBox {
        tenc_box: Some(arb_tenc_box(ctx)),
        unknown_boxes: sample_vec(ctx, 0..3, arb_unknown_box),
    }
}

/// パターン暗号化を使うスキーム (`cens` / `cbcs`) かどうかを返す
fn uses_pattern_encryption(scheme_type: [u8; 4]) -> bool {
    matches!(&scheme_type, b"cens" | b"cbcs")
}

/// スキームと整合する TencBox を生成する
///
/// `cens` / `cbcs` はパターン暗号化が必須なので version 1 のパラメーターを必ず付け、
/// `cenc` / `cbc1` はパターン暗号化を禁止しているので付けない。
fn arb_tenc_box_for_scheme(ctx: &mut TestCaseContext, scheme_type: [u8; 4]) -> TencBox {
    let mut tenc = arb_tenc_box(ctx);
    if uses_pattern_encryption(scheme_type) {
        tenc.default_crypt_byte_block = Some(Uint::new(noprop::sample_u64_in(ctx, 0..=0x0F) as u8));
        tenc.default_skip_byte_block = Some(Uint::new(noprop::sample_u64_in(ctx, 0..=0x0F) as u8));
    } else {
        tenc.default_crypt_byte_block = None;
        tenc.default_skip_byte_block = None;
    }
    tenc
}

/// SinfBox を生成する
///
/// `frma` は固定値とし、`schm` / `schi` は CENC 系スキーム (`cenc` / `cbc1` / `cens` / `cbcs`)
/// で常に揃える。`schi` にはスキームと整合した `tenc` を持たせる。
fn arb_sinf_box(ctx: &mut TestCaseContext) -> SinfBox {
    let scheme_type = match noprop::sample_u64_in(ctx, 0..=3) {
        0 => *b"cenc",
        1 => *b"cbc1",
        2 => *b"cens",
        _ => *b"cbcs",
    };
    SinfBox {
        frma_box: FrmaBox {
            data_format: *b"avc1",
        },
        schm_box: Some(SchmBox {
            scheme_type,
            scheme_version: noprop::sample_u32(ctx),
            scheme_uri: None,
        }),
        schi_box: Some(SchiBox {
            tenc_box: Some(arb_tenc_box_for_scheme(ctx, scheme_type)),
            unknown_boxes: Vec::new(),
        }),
        unknown_boxes: Vec::new(),
    }
}

// ===== PsshBox のテスト =====

/// PsshBox (version 0) の encode/decode roundtrip
#[test]
fn pssh_box_v0_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let pssh = arb_pssh_box_v0(ctx);
        let encoded = pssh.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = PsshBox::decode(&encoded)
            .expect("直前にエンコードした有効な PsshBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded, pssh);
        Ok(())
    })?;
    Ok(())
}

/// PsshBox (version 1) の encode/decode roundtrip
///
/// KID 0 件（ムービー内の全 KID に適用する表現）も含めて検証する。
#[test]
fn pssh_box_v1_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let pssh = arb_pssh_box_v1(ctx);
        let encoded = pssh.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = PsshBox::decode(&encoded)
            .expect("直前にエンコードした有効な PsshBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded, pssh);
        Ok(())
    })?;
    Ok(())
}

// ===== FrmaBox のテスト =====

/// FrmaBox の encode/decode roundtrip
#[test]
fn frma_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let frma = arb_frma_box(ctx);
        let encoded = frma.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = FrmaBox::decode(&encoded)
            .expect("直前にエンコードした有効な FrmaBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded, frma);
        Ok(())
    })?;
    Ok(())
}

// ===== SchmBox のテスト =====

/// SchmBox の encode/decode roundtrip
///
/// `scheme_uri` あり・なしの両方と、そのときの flags の往復を検証する。
#[test]
fn schm_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let schm = arb_schm_box(ctx);
        let encoded = schm.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SchmBox::decode(&encoded)
            .expect("直前にエンコードした有効な SchmBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded, schm);
        Ok(())
    })?;
    Ok(())
}

// ===== SchiBox のテスト =====

/// SchiBox の encode/decode roundtrip
///
/// `tenc` と未知の子ボックスを同時に保持したまま往復できることを検証する。
#[test]
fn schi_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let schi = arb_schi_box(ctx);
        let encoded = schi.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SchiBox::decode(&encoded)
            .expect("直前にエンコードした有効な SchiBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded, schi);
        Ok(())
    })?;
    Ok(())
}

// ===== SinfBox のテスト =====

/// SinfBox の encode/decode roundtrip
///
/// `cenc` / `cbc1` はパターン暗号化なし、`cens` / `cbcs` はパターン暗号化ありの
/// `tenc` を組み合わせて検証する。
#[test]
fn sinf_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let sinf = arb_sinf_box(ctx);
        let encoded = sinf.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = SinfBox::decode(&encoded)
            .expect("直前にエンコードした有効な SinfBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded, sinf);
        Ok(())
    })?;
    Ok(())
}

// ===== TencBox のテスト =====

/// TencBox の encode/decode roundtrip
///
/// version 0 / 1 と constant IV あり・なしの全組み合わせを検証する。
#[test]
fn tenc_box_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let tenc = arb_tenc_box(ctx);
        let encoded = tenc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, size) = TencBox::decode(&encoded)
            .expect("直前にエンコードした有効な TencBox は必ずデコードできる");

        assert_eq!(size, encoded.len());
        assert_eq!(decoded, tenc);
        Ok(())
    })?;
    Ok(())
}
