//! ディスクリプター構造体の Property-Based Testing

use noprop::TestCaseContext;
use shiguredo_mp4::{
    Decode, Encode, Uint,
    descriptors::{DecoderConfigDescriptor, DecoderSpecificInfo, EsDescriptor, SlConfigDescriptor},
};

/// このファイルの PBT ケース数（旧 `with_cases(200)` を維持）
const CASES: usize = 200;

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
///
/// ペイロード長は、サイズ欄が 1 バイトになる 127 以下と 2 バイトになる 128 以上の両方を含む `0..300` から選ぶ
fn arb_decoder_specific_info(ctx: &mut TestCaseContext) -> DecoderSpecificInfo {
    let len = noprop::sample_usize_in(ctx, 0..300);
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

/// 順に並べるディスクリプターの種類と値
#[derive(Debug, PartialEq, Eq)]
enum AnyDescriptor {
    Es(EsDescriptor),
    DecoderConfig(DecoderConfigDescriptor),
    DecoderSpecificInfo(DecoderSpecificInfo),
    SlConfig(SlConfigDescriptor),
}

impl AnyDescriptor {
    /// エンコードしたバイト列を返す（失敗した場合はパニックする）
    fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::Es(v) => v.encode_to_vec(),
            Self::DecoderConfig(v) => v.encode_to_vec(),
            Self::DecoderSpecificInfo(v) => v.encode_to_vec(),
            Self::SlConfig(v) => v.encode_to_vec(),
        }
        .expect("Vec への書き込みは失敗しない")
    }

    /// `buf` の `offset` から自身と同じ種類のディスクリプターをデコードし、`offset` を進める（デコードに失敗した場合はパニックする）
    fn decode_same_kind_at(&self, buf: &[u8], offset: &mut usize) -> Self {
        match self {
            Self::Es(_) => Self::Es(
                EsDescriptor::decode_at(buf, offset).expect("有効な EsDescriptor はデコードできる"),
            ),
            Self::DecoderConfig(_) => Self::DecoderConfig(
                DecoderConfigDescriptor::decode_at(buf, offset)
                    .expect("有効な DecoderConfigDescriptor はデコードできる"),
            ),
            Self::DecoderSpecificInfo(_) => Self::DecoderSpecificInfo(
                DecoderSpecificInfo::decode_at(buf, offset)
                    .expect("有効な DecoderSpecificInfo はデコードできる"),
            ),
            Self::SlConfig(_) => Self::SlConfig(
                SlConfigDescriptor::decode_at(buf, offset)
                    .expect("有効な SlConfigDescriptor はデコードできる"),
            ),
        }
    }
}

/// 4 種類のディスクリプターのいずれかを生成する
fn arb_any_descriptor(ctx: &mut TestCaseContext) -> AnyDescriptor {
    match noprop::sample_usize_in(ctx, 0..4) {
        0 => AnyDescriptor::Es(arb_es_descriptor(ctx)),
        1 => AnyDescriptor::DecoderConfig(arb_decoder_config_descriptor(ctx)),
        2 => AnyDescriptor::DecoderSpecificInfo(arb_decoder_specific_info(ctx)),
        _ => AnyDescriptor::SlConfig(SlConfigDescriptor),
    }
}

/// エンコード済みのディスクリプター 1 つ分のバイト列を、タグとペイロードに分ける
///
/// `encoded` はちょうど 1 つのディスクリプターであること（宣言サイズは見ずに、ヘッダーより後をすべてペイロードとする）
fn split_descriptor(encoded: &[u8]) -> (u8, &[u8]) {
    // サイズ欄は継続ビットが立っていないバイトで終わる
    let mut offset = 1;
    while encoded[offset] & 0x80 != 0 {
        offset += 1;
    }
    (encoded[0], &encoded[offset + 1..])
}

/// ペイロードにタグとサイズ欄を付ける
///
/// サイズ欄の長さは、ペイロード長を表すのに必要な最小のバイト数から 4 バイトまでの間でランダムに選ぶ。
/// サイズ欄は上位から 7 ビットずつ並べ、最後のバイト以外に継続ビットを立てる。
/// 最小より長い場合は、先頭に値 0 で継続ビットだけが立ったバイト（0x80）が並ぶ。
/// ペイロード長は 4 バイトのサイズ欄で表せる（2 の 28 乗未満である）こと
fn wrap_payload(ctx: &mut TestCaseContext, tag: u8, payload: &[u8]) -> Vec<u8> {
    let mut min_size_field_len = 1;
    while payload.len() >> (7 * min_size_field_len) != 0 {
        min_size_field_len += 1;
    }
    let size_field_len = noprop::sample_usize_in(ctx, min_size_field_len..=4);

    let mut encoded = vec![tag];
    for i in (0..size_field_len).rev() {
        let bits = ((payload.len() >> (7 * i)) & 0x7F) as u8;
        let continuation = if i > 0 { 0x80 } else { 0 };
        encoded.push(bits | continuation);
    }
    encoded.extend_from_slice(payload);
    encoded
}

/// デコーダーが解釈しない、宣言されたペイロードの末尾のバイト列を生成する
///
/// `avoid_dec_specific_info_tag` が真の場合は、先頭が DecoderSpecificInfo のタグ（5）にならないようにする
fn arb_unparsed_trailing(ctx: &mut TestCaseContext, avoid_dec_specific_info_tag: bool) -> Vec<u8> {
    let len = noprop::sample_usize_in(ctx, 1..20);
    let mut trailing = noprop::sample_bytes_vec(ctx, len);
    if avoid_dec_specific_info_tag && trailing[0] == 0x05 {
        // DecoderSpecificInfo を持たない DecoderConfigDescriptor では、足した末尾が 5 で始まると
        // DecoderSpecificInfo として読まれる。
        // 解釈しない末尾にするため、DecoderSpecificInfo のタグではない値に置き換える
        trailing[0] = 0x14;
    }
    trailing
}

/// エンコード済みのディスクリプター 1 つ分の宣言されたペイロードの末尾に、解釈しないバイト列を足してヘッダーを付け直す
///
/// `avoid_dec_specific_info_tag` は [`arb_unparsed_trailing`] にそのまま渡す
fn append_unparsed_trailing(
    ctx: &mut TestCaseContext,
    encoded: &[u8],
    avoid_dec_specific_info_tag: bool,
) -> Vec<u8> {
    let (tag, payload) = split_descriptor(encoded);
    let mut payload = payload.to_vec();
    payload.extend(arb_unparsed_trailing(ctx, avoid_dec_specific_info_tag));
    wrap_payload(ctx, tag, &payload)
}

/// EsDescriptor を、自身と子の宣言されたペイロードの末尾に解釈しないバイト列を足してエンコードする
///
/// 子の DecoderConfigDescriptor と SlConfigDescriptor、自身のそれぞれについて、末尾を足すかどうかをランダムに選ぶ。
/// どこにも足さない場合も、自身のサイズ欄はランダムな長さで付け直す
fn encode_es_with_unparsed_trailing(
    ctx: &mut TestCaseContext,
    descriptor: &EsDescriptor,
) -> Vec<u8> {
    let encoded = descriptor
        .encode_to_vec()
        .expect("Vec への書き込みは失敗しない");
    let (tag, payload) = split_descriptor(&encoded);

    // EsDescriptor のペイロードは、固定と任意のフィールド、DecoderConfigDescriptor、SlConfigDescriptor の順に並ぶ
    let dcd = descriptor
        .dec_config_descr
        .encode_to_vec()
        .expect("Vec への書き込みは失敗しない");
    let sl = descriptor
        .sl_config_descr
        .encode_to_vec()
        .expect("Vec への書き込みは失敗しない");
    let fields_len = payload.len() - dcd.len() - sl.len();

    let mut new_payload = payload[..fields_len].to_vec();
    if noprop::sample_bool(ctx) {
        let avoid_dec_specific_info_tag = descriptor.dec_config_descr.dec_specific_info.is_none();
        new_payload.extend(append_unparsed_trailing(
            ctx,
            &dcd,
            avoid_dec_specific_info_tag,
        ));
    } else {
        new_payload.extend(dcd);
    }
    if noprop::sample_bool(ctx) {
        new_payload.extend(append_unparsed_trailing(ctx, &sl, false));
    } else {
        new_payload.extend(sl);
    }
    if noprop::sample_bool(ctx) {
        new_payload.extend(arb_unparsed_trailing(ctx, false));
    }
    wrap_payload(ctx, tag, &new_payload)
}

/// ディスクリプターを、自身や子の宣言されたペイロードの末尾に解釈しないバイト列を足してエンコードする
///
/// DecoderSpecificInfo はペイロード全体が値なので、末尾を足さずにそのままエンコードする
fn encode_with_unparsed_trailing(ctx: &mut TestCaseContext, descriptor: &AnyDescriptor) -> Vec<u8> {
    match descriptor {
        AnyDescriptor::Es(v) => encode_es_with_unparsed_trailing(ctx, v),
        AnyDescriptor::DecoderConfig(v) => {
            append_unparsed_trailing(ctx, &descriptor.to_bytes(), v.dec_specific_info.is_none())
        }
        AnyDescriptor::DecoderSpecificInfo(_) => descriptor.to_bytes(),
        AnyDescriptor::SlConfig(_) => append_unparsed_trailing(ctx, &descriptor.to_bytes(), false),
    }
}

/// 連結したディスクリプターを順にデコードでき、各消費バイト数が次のディスクリプターの先頭を指すこと
///
/// 一部のディスクリプターでは、自身や子の宣言されたペイロードの末尾に解釈しないバイト列を足し、
/// サイズ欄を必要以上の長さで表すこともある。その場合も値が変わらず、消費バイト数がヘッダー長と
/// 宣言サイズの合計になることを確認する。
/// また、ケースのおよそ半分で DecoderSpecificInfo を持たない DecoderConfigDescriptor の直後に
/// DecoderSpecificInfo を並べ、それを子として読まないことも確認する
#[test]
fn descriptors_decode_sequentially() -> noprop::TestResult {
    let seed = noprop::seed_from_env_or_time("MP4_RS_PBT_SEED")?;
    noprop::Runner::new(seed).run(CASES, |ctx| {
        let count = noprop::sample_usize_in(ctx, 1..6);
        let mut descriptors = Vec::new();
        for _ in 0..count {
            descriptors.push(arb_any_descriptor(ctx));
        }
        if noprop::sample_bool(ctx) {
            // 任意の子の有無を宣言されたペイロードの外で判定すると、直後の DecoderSpecificInfo を子として読んでしまう
            let mut dcd = arb_decoder_config_descriptor(ctx);
            dcd.dec_specific_info = None;
            let position = noprop::sample_usize_in(ctx, 0..=descriptors.len());
            descriptors.insert(position, AnyDescriptor::DecoderConfig(dcd));
            descriptors.insert(
                position + 1,
                AnyDescriptor::DecoderSpecificInfo(arb_decoder_specific_info(ctx)),
            );
        }

        let mut buf = Vec::new();
        // 各ディスクリプターの終端（次のディスクリプターの先頭）の位置
        let mut ends = Vec::new();
        for descriptor in &descriptors {
            let encoded = if noprop::sample_bool(ctx) {
                encode_with_unparsed_trailing(ctx, descriptor)
            } else {
                descriptor.to_bytes()
            };
            buf.extend_from_slice(&encoded);
            ends.push(buf.len());
        }

        let mut offset = 0;
        for (descriptor, end) in descriptors.iter().zip(ends) {
            let decoded = descriptor.decode_same_kind_at(&buf, &mut offset);
            assert_eq!(&decoded, descriptor);
            assert_eq!(
                offset, end,
                "消費バイト数が次のディスクリプターの先頭を指す"
            );
        }
        Ok(())
    })?;
    Ok(())
}
