//! ディスクリプター構造体の Property-Based Testing

use noprop::TestCaseContext;
use shiguredo_mp4::{
    Decode, Encode, Uint,
    descriptors::{DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor},
};

/// このファイルの PBT ケース数（旧 `with_cases(200)` を維持）
const CASES: usize = 200;

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

/// ASCII 英数字（`[a-zA-Z0-9]`）を最大 `max_len` 文字まで生成する
fn arb_ascii_alphanumeric(ctx: &mut TestCaseContext, max_len: usize) -> String {
    let len = noprop::sample_usize_in(ctx, 0..=max_len);
    let mut s = String::new();
    for _ in 0..len {
        // ASCII 英数字は a-z + A-Z + 0-9 = 62 種類
        let idx = noprop::sample_usize_in(ctx, 0..62);
        let c = if idx < 26 {
            (b'a' + idx as u8) as char
        } else if idx < 52 {
            (b'A' + (idx - 26) as u8) as char
        } else {
            (b'0' + (idx - 52) as u8) as char
        };
        s.push(c);
    }
    s
}

/// DecoderSpecificInfo を生成する
fn arb_decoder_specific_info(ctx: &mut TestCaseContext) -> DecoderSpecificInfo {
    let len = noprop::sample_usize_in(ctx, 0..50);
    let payload = noprop::sample_bytes_vec(ctx, len);
    DecoderSpecificInfo { payload }
}

/// DecoderConfigDescriptor を生成する
fn arb_decoder_config_descriptor(ctx: &mut TestCaseContext) -> DecoderConfigDescriptor {
    let object_type_indication = noprop::sample_u8(ctx);
    let stream_type = noprop::sample_u64_in(ctx, 0..64) as u8; // 6 bits
    let up_stream = noprop::sample_bool(ctx);
    let buffer_size_db = noprop::sample_u32(ctx) & 0x00FF_FFFF; // 24 bits
    let max_bitrate = noprop::sample_u32(ctx);
    let avg_bitrate = noprop::sample_u32(ctx);
    let dec_specific_info = if noprop::sample_bool(ctx) {
        Some(arb_decoder_specific_info(ctx))
    } else {
        None
    };
    DecoderConfigDescriptor {
        object_type_indication,
        stream_type: Uint::new(stream_type),
        up_stream: Uint::new(up_stream as u8),
        buffer_size_db: Uint::new(buffer_size_db),
        max_bitrate,
        avg_bitrate,
        dec_specific_info,
    }
}

/// EsDescriptor を生成する
fn arb_es_descriptor(ctx: &mut TestCaseContext) -> EsDescriptor {
    let es_id = noprop::sample_u64_in(ctx, 1..=u16::MAX as u64) as u16;
    let stream_priority = noprop::sample_u64_in(ctx, 0..32) as u8; // 5 bits
    let depends_on_es_id = if noprop::sample_bool(ctx) {
        Some(noprop::sample_u64_in(ctx, 1..=u16::MAX as u64) as u16)
    } else {
        None
    };
    let url_string = if noprop::sample_bool(ctx) {
        Some(arb_ascii_alphanumeric(ctx, 20))
    } else {
        None
    };
    let ocr_es_id = if noprop::sample_bool(ctx) {
        Some(noprop::sample_u64_in(ctx, 1..=u16::MAX as u64) as u16)
    } else {
        None
    };
    let dec_config_descr = arb_decoder_config_descriptor(ctx);
    EsDescriptor {
        es_id,
        stream_priority: Uint::new(stream_priority),
        depends_on_es_id,
        url_string,
        ocr_es_id,
        dec_config_descr,
        sl_config_descr: SlConfigDescriptor,
    }
}

// ===== DecoderSpecificInfo のテスト =====

/// DecoderSpecificInfo の encode/decode roundtrip
#[test]
fn decoder_specific_info_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let payload = sample_vec(ctx, 0..100, noprop::sample_u8);
        let info = DecoderSpecificInfo {
            payload: payload.clone(),
        };
        let encoded = info.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = DecoderSpecificInfo::decode(&encoded)
            .expect("直前にエンコードした有効な DecoderSpecificInfo は必ずデコードできる");

        assert_eq!(decoded.payload, payload);
        Ok(())
    })?;
    Ok(())
}

// ===== DecoderConfigDescriptor のテスト =====

/// DecoderConfigDescriptor の encode/decode roundtrip
#[test]
fn decoder_config_descriptor_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let desc = arb_decoder_config_descriptor(ctx);
        let encoded = desc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = DecoderConfigDescriptor::decode(&encoded)
            .expect("直前にエンコードした有効な DecoderConfigDescriptor は必ずデコードできる");

        assert_eq!(decoded.object_type_indication, desc.object_type_indication);
        assert_eq!(decoded.stream_type.get(), desc.stream_type.get());
        assert_eq!(decoded.up_stream.get(), desc.up_stream.get());
        assert_eq!(decoded.buffer_size_db.get(), desc.buffer_size_db.get());
        assert_eq!(decoded.max_bitrate, desc.max_bitrate);
        assert_eq!(decoded.avg_bitrate, desc.avg_bitrate);
        assert_eq!(decoded.dec_specific_info, desc.dec_specific_info);
        Ok(())
    })?;
    Ok(())
}

// ===== EsDescriptor のテスト =====

/// EsDescriptor の encode/decode roundtrip
#[test]
fn es_descriptor_roundtrip() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let desc = arb_es_descriptor(ctx);
        let encoded = desc.encode_to_vec().expect("Vec への書き込みは失敗しない");
        let (decoded, _) = EsDescriptor::decode(&encoded)
            .expect("直前にエンコードした有効な EsDescriptor は必ずデコードできる");

        assert_eq!(decoded.es_id, desc.es_id);
        assert_eq!(decoded.stream_priority.get(), desc.stream_priority.get());
        assert_eq!(decoded.depends_on_es_id, desc.depends_on_es_id);
        assert_eq!(decoded.url_string, desc.url_string);
        assert_eq!(decoded.ocr_es_id, desc.ocr_es_id);
        assert_eq!(
            decoded.dec_config_descr.object_type_indication,
            desc.dec_config_descr.object_type_indication
        );
        assert_eq!(
            decoded.dec_config_descr.stream_type.get(),
            desc.dec_config_descr.stream_type.get()
        );
        assert_eq!(
            decoded.dec_config_descr.max_bitrate,
            desc.dec_config_descr.max_bitrate
        );
        assert_eq!(
            decoded.dec_config_descr.avg_bitrate,
            desc.dec_config_descr.avg_bitrate
        );
        Ok(())
    })?;
    Ok(())
}
