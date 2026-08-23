use proptest::prelude::*;
use wasm_runtime::{
    module::parse_module,
    parser::{decode_leb128_i32, decode_leb128_i64, decode_leb128_u32, section_iter},
};

proptest! {
    /// decode_leb128_u32 は任意バイト列でパニックしない。
    #[test]
    fn leb128_u32_no_panic(data in proptest::collection::vec(any::<u8>(), 0..32)) {
        let _ = decode_leb128_u32(&data, 0);
    }

    /// オフセット指定でもパニックしない。
    #[test]
    fn leb128_u32_offset_no_panic(
        data in proptest::collection::vec(any::<u8>(), 0..32),
        offset in 0usize..64,
    ) {
        let _ = decode_leb128_u32(&data, offset);
    }

    /// decode_leb128_i32 は任意バイト列でパニックしない。
    #[test]
    fn leb128_i32_no_panic(data in proptest::collection::vec(any::<u8>(), 0..32)) {
        let _ = decode_leb128_i32(&data, 0);
    }

    /// decode_leb128_i64 は任意バイト列でパニックしない。
    #[test]
    fn leb128_i64_no_panic(data in proptest::collection::vec(any::<u8>(), 0..32)) {
        let _ = decode_leb128_i64(&data, 0);
    }

    /// section_iter は任意バイト列をイテレートしてもパニックしない。
    #[test]
    fn section_iter_no_panic(data in proptest::collection::vec(any::<u8>(), 0..256)) {
        for result in section_iter(&data) {
            let _ = result;
        }
    }

    /// parse_module は任意バイト列でパニックしない (Ok か Err のみ返す)。
    #[test]
    fn parse_module_no_panic(data in proptest::collection::vec(any::<u8>(), 0..256)) {
        let _ = parse_module(&data);
    }

    /// 正しい LEB128 エンコード値は同じ値にデコードされる (u32 往復)。
    #[test]
    fn leb128_u32_roundtrip(value in 0u32..=u32::MAX) {
        let encoded = encode_leb128_u32(value);
        let (decoded, _) = decode_leb128_u32(&encoded, 0).expect("valid encoding must decode");
        prop_assert_eq!(decoded, value);
    }

    /// 正しい LEB128 エンコード値は同じ値にデコードされる (i32 往復)。
    #[test]
    fn leb128_i32_roundtrip(value in i32::MIN..=i32::MAX) {
        let encoded = encode_leb128_i32(value);
        let (decoded, _) = decode_leb128_i32(&encoded, 0).expect("valid encoding must decode");
        prop_assert_eq!(decoded, value);
    }
}

/// 標準 LEB128 unsigned エンコーダ（テスト専用）。
fn encode_leb128_u32(mut value: u32) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
    out
}

/// 標準 LEB128 signed エンコーダ（テスト専用）。
fn encode_leb128_i32(mut value: i32) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let byte = (value & 0x7F) as u8;
        value >>= 7; // arithmetic right shift
        let done = (value == 0 && (byte & 0x40) == 0) || (value == -1 && (byte & 0x40) != 0);
        if done {
            out.push(byte);
            break;
        } else {
            out.push(byte | 0x80);
        }
    }
    out
}
