//! MP4 の特定の話題についての補足ドキュメント
//!
//! 個々の型やボックスの説明ではなく、複数のコンポーネントにまたがる話題をまとめている。

// ドキュメントの本体は docs/ 配下の Markdown ファイルに置き、include_str! で取り込んでいる。
// こうすることで docs.rs から参照できるようになり、
// 本文中の Rust コード例が doctest として検証されるようになる。

/// Hybrid MP4 の取り扱いについての補足ドキュメント
#[doc = include_str!("../docs/hybrid_mp4.md")]
pub mod hybrid_mp4 {}

/// 字幕トラックの取り扱いについての補足ドキュメント
#[doc = include_str!("../docs/subtitle.md")]
pub mod subtitle {}
