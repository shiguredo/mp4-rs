//! MP4 の仕様とは直接は関係がない、実装上便利な補助的なコンポーネントを集めたモジュール
use alloc::vec::Vec;
use core::num::NonZeroU32;

use crate::{
    BoxType, Either,
    boxes::{CttsBox, SampleEntry, StblBox, StscBox, StscEntry, StszBox, SttsBox},
};

/// [`StblBox`] をラップして、その中の情報を簡単かつ効率的に取り出せるようにするための構造体
#[derive(Debug, Clone)]
pub struct SampleTableAccessor<T> {
    stbl_box: T,
    chunk_count: u32,
    sample_count: u32,
    sample_durations: Vec<(u32, u32, u64)>, // (累計サンプル数、尺、累計尺）
    sample_composition_offsets: Option<Vec<(u32, i64)>>, // (累計サンプル数、オフセット)
    sample_index_offsets: Vec<NonZeroU32>,  // チャンク先頭のサンプルインデックス
    sample_data_offsets: Vec<u64>,
}

impl<T: AsRef<StblBox>> SampleTableAccessor<T> {
    /// 引数で渡された [`StblBox`] 用の [`SampleTableAccessor`] インスタンスを生成する
    pub fn new(stbl_box: T) -> Result<Self, SampleTableAccessorError> {
        let stbl_box_ref = stbl_box.as_ref();
        let mut sample_count: u32 = 0;
        let mut sample_durations = Vec::new();
        let mut acc_duration = 0;
        for entry in &stbl_box_ref.stts_box.entries {
            sample_durations.push((sample_count, entry.sample_delta, acc_duration));
            // sample_count の checked_add により Σ sample_count <= u32::MAX が保証され、
            // acc_duration <= (2^32 - 1) * (2^32 - 1) = 18446744065119617025 < u64::MAX
            // が成り立つため、acc_duration の加算はオーバーフローしない。
            // この関係は sample_count の加算を acc_duration の加算より先に行うことに
            // 依存するため、2 つの加算の順序自体が仕様である。
            sample_count = sample_count.checked_add(entry.sample_count).ok_or(
                SampleTableAccessorError::SampleCountOverflow {
                    box_type: SttsBox::TYPE,
                    accumulated_sample_count: sample_count,
                    entry_sample_count: entry.sample_count,
                },
            )?;
            acc_duration += entry.sample_delta as u64 * entry.sample_count as u64;
        }

        // Variable / Fixed のいずれでも stts 合計と stsz のサンプル数を突き合わせる。
        // Fixed はワイヤ上の sample_count を、Variable は entry_sizes.len() を使う。
        // stsc 検査より前に置くことで、同時に食い違うときの表面化順序を両バリアントで揃える。
        match &stbl_box_ref.stsz_box {
            StszBox::Variable { entry_sizes } => {
                if entry_sizes.len() != sample_count as usize {
                    return Err(SampleTableAccessorError::InconsistentSampleCount {
                        stts_sample_count: sample_count,
                        other_box_type: StszBox::TYPE,
                        other_sample_count: entry_sizes.len() as u32,
                    });
                }
            }
            StszBox::Fixed {
                sample_count: stsz_sample_count,
                ..
            } => {
                if *stsz_sample_count != sample_count {
                    return Err(SampleTableAccessorError::InconsistentSampleCount {
                        stts_sample_count: sample_count,
                        other_box_type: StszBox::TYPE,
                        other_sample_count: *stsz_sample_count,
                    });
                }
            }
        }

        let sample_composition_offsets = if let Some(ctts_box) = &stbl_box_ref.ctts_box {
            let mut ctts_sample_count: u32 = 0;
            let mut sample_composition_offsets = Vec::new();
            for entry in &ctts_box.entries {
                sample_composition_offsets.push((ctts_sample_count, entry.sample_offset));
                ctts_sample_count = ctts_sample_count.checked_add(entry.sample_count).ok_or(
                    SampleTableAccessorError::SampleCountOverflow {
                        box_type: CttsBox::TYPE,
                        accumulated_sample_count: ctts_sample_count,
                        entry_sample_count: entry.sample_count,
                    },
                )?;
            }
            if ctts_sample_count != sample_count {
                return Err(SampleTableAccessorError::InconsistentSampleCount {
                    stts_sample_count: sample_count,
                    other_box_type: CttsBox::TYPE,
                    other_sample_count: ctts_sample_count,
                });
            }
            Some(sample_composition_offsets)
        } else {
            None
        };

        let chunk_count = match &stbl_box_ref.stco_or_co64_box {
            Either::A(b) => b.chunk_offsets.len() as u32,
            Either::B(b) => b.chunk_offsets.len() as u32,
        };

        if chunk_count > 0 && stbl_box_ref.stsc_box.entries.is_empty() {
            // チャンクは存在するのに stsc エントリーが空のケース
            return Err(SampleTableAccessorError::ChunksExistButNoSamples { chunk_count });
        }

        if let Some(x) = stbl_box_ref.stsc_box.entries.first()
            && x.first_chunk.get() != 1
        {
            // チャンクインデックスが 1 以外から始まっている
            return Err(SampleTableAccessorError::FirstChunkIndexIsNotOne {
                actual_chunk_index: x.first_chunk,
            });
        }
        if let Some(i) = stbl_box_ref.stsc_box.entries.iter().position(|x| {
            stbl_box_ref.stsd_box.entries.len() < x.sample_description_index.get() as usize
        }) {
            // 存在しないサンプルエントリーを参照しているチャンクがある
            return Err(SampleTableAccessorError::MissingSampleEntry {
                stsc_entry_index: i,
                sample_description_index: stbl_box_ref.stsc_box.entries[i].sample_description_index,
                sample_entry_count: stbl_box_ref.stsd_box.entries.len(),
            });
        }
        if stbl_box_ref
            .stsc_box
            .entries
            .iter()
            .zip(stbl_box_ref.stsc_box.entries.iter().skip(1))
            .any(|(prev, next)| prev.first_chunk >= next.first_chunk)
        {
            // stsc 内のチャンクインデックスが短調増加していない
            return Err(SampleTableAccessorError::ChunkIndicesNotMonotonicallyIncreasing);
        }
        if let Some(max_chunk_index) = NonZeroU32::new(chunk_count)
            && let Some(last) = stbl_box_ref
                .stsc_box
                .entries
                .last()
                .filter(|x| max_chunk_index < x.first_chunk)
        {
            // stco / co64 のチャンク数と stsc のチャンク数が一致していない
            return Err(SampleTableAccessorError::LastChunkIndexIsTooLarge {
                max_chunk_index,
                last_chunk_index: last.first_chunk,
            });
        }

        let mut sample_index_offsets = Vec::new();
        let mut first_sample_index = NonZeroU32::MIN;
        for i in 0..chunk_count {
            let chunk_index = NonZeroU32::MIN.saturating_add(i);
            sample_index_offsets.push(first_sample_index);

            let j = stbl_box_ref
                .stsc_box
                .entries
                .binary_search_by_key(&chunk_index, |x| x.first_chunk)
                .unwrap_or_else(|j| j - 1);
            first_sample_index = first_sample_index
                .saturating_add(stbl_box_ref.stsc_box.entries[j].sample_per_chunk);
        }
        if first_sample_index.get() - 1 != sample_count {
            // stts と stsc でサンプル数が異なる
            return Err(SampleTableAccessorError::InconsistentSampleCount {
                stts_sample_count: sample_count,
                other_box_type: StscBox::TYPE,
                other_sample_count: first_sample_index.get() - 1,
            });
        }

        let mut this = Self {
            stbl_box,
            chunk_count,
            sample_count,
            sample_durations,
            sample_composition_offsets,
            sample_index_offsets,
            sample_data_offsets: Vec::new(),
        };

        // Fixed はテーブルを持たず overflow 検出のみ。Variable は prefix-sum を構築する。
        // 詳細は各メソッド先頭のコメントを参照。
        if let StszBox::Fixed { sample_size, .. } = &this.stbl_box().stsz_box {
            this.validate_fixed_sample_data_offsets(*sample_size)?;
        } else {
            this.sample_data_offsets = this.build_variable_sample_data_offsets()?;
        }

        Ok(this)
    }

    // stsz が Fixed のときは全サンプル同一サイズなので、sample_data_offsets
    // テーブルを構築せず data_offset() を算術で算出する。テーブルを構築すると
    // 入力サイズ（stsz はワイヤ上 8 バイト）と乖離した最大約 34 GB の確保に
    // 到達できるためである。このパスではオーバーフロー検出だけをチャンク単位で
    // 行う（チャンク数は stco / co64 の配列長に等しく、入力サイズに比例する）。
    fn validate_fixed_sample_data_offsets(
        &self,
        sample_size: NonZeroU32,
    ) -> Result<(), SampleTableAccessorError> {
        let s = sample_size.get() as u64;
        for (chunk_index, chunk) in self.chunks().enumerate() {
            let base = chunk.offset();
            let k = chunk.sample_count() as u64;
            // eager ループは、チャンク内の floor((u64::MAX - base) / s) + 1 番目
            // （0 始まりの j 番目）のサンプルで最初にオーバーフローする。
            // 判定は k > (u64::MAX - base) / s で済み、s >= 1 なので除算は安全
            if k > (u64::MAX - base) / s {
                let j = (u64::MAX - base) / s;
                // j < k <= u32::MAX なので j は u32 に収まり、先頭サンプルインデックス
                // との和は sample_count 以下になる（前述の stsc 突き合わせで保証済み）
                let sample_index =
                    NonZeroU32::new(self.sample_index_offsets[chunk_index].get() + j as u32)
                        .expect("overflowing sample index is always within sample_count");
                return Err(SampleTableAccessorError::SampleDataOffsetOverflow {
                    sample_index,
                    accumulated_offset: base + j * s,
                    sample_data_size: sample_size.get(),
                });
            }
        }
        Ok(())
    }

    // Variable は entry_sizes がワイヤ上にサンプル数比例で存在するため、
    // prefix-sum テーブルを構築しても入力サイズから大きく乖離しない。
    fn build_variable_sample_data_offsets(&self) -> Result<Vec<u64>, SampleTableAccessorError> {
        let mut sample_data_offsets = Vec::new();
        for chunk in self.chunks() {
            let mut offset = chunk.offset();
            for sample in chunk.samples() {
                sample_data_offsets.push(offset);
                offset = offset.checked_add(sample.data_size() as u64).ok_or(
                    SampleTableAccessorError::SampleDataOffsetOverflow {
                        sample_index: sample.index(),
                        accumulated_offset: offset,
                        sample_data_size: sample.data_size(),
                    },
                )?;
            }
        }
        Ok(sample_data_offsets)
    }

    /// トラック内のサンプルの数を取得する
    pub fn sample_count(&self) -> u32 {
        self.sample_count
    }

    /// トラック内のチャンクの数を取得する
    pub fn chunk_count(&self) -> u32 {
        self.chunk_count
    }

    /// 指定されたサンプルの情報を返す
    ///
    /// 存在しないサンプルが指定された場合には [`None`] が返される
    pub fn get_sample(&self, sample_index: NonZeroU32) -> Option<SampleAccessor<'_, T>> {
        (sample_index.get() <= self.sample_count).then_some(SampleAccessor {
            sample_table: self,
            index: sample_index,
        })
    }

    /// 指定されたタイムスタンプ（トラック先頭からの累計尺）を含むサンプルの情報を返す
    ///
    /// 該当のサンプルが存在しない場合には [`None`] が返される
    pub fn get_sample_by_timestamp(&self, timestamp: u64) -> Option<SampleAccessor<'_, T>> {
        let mut low = 0;
        let mut high = self.sample_count;
        while high > low {
            let i = (high - low) / 2 + low;
            let sample = SampleAccessor {
                sample_table: self,
                index: NonZeroU32::MIN.saturating_add(i),
            };
            let sample_timestamp = sample.timestamp();

            match timestamp.cmp(&sample_timestamp) {
                core::cmp::Ordering::Less => {
                    high = i;
                }
                core::cmp::Ordering::Equal => return Some(sample),
                core::cmp::Ordering::Greater => {
                    if timestamp < sample_timestamp + sample.duration() as u64 {
                        return Some(sample);
                    }
                    low = i + 1;
                }
            }
        }
        None
    }

    /// 指定されたチャンクの情報を返す
    ///
    /// 存在しないチャンクが指定された場合には [`None`] が返される
    pub fn get_chunk(&self, chunk_index: NonZeroU32) -> Option<ChunkAccessor<'_, T>> {
        (chunk_index.get() <= self.chunk_count()).then_some(ChunkAccessor {
            sample_table: self,
            index: chunk_index,
        })
    }

    /// トラック内のサンプル群の情報を走査するイテレーターを返す
    pub fn samples(&self) -> impl '_ + Iterator<Item = SampleAccessor<'_, T>> {
        (0..self.sample_count()).map(|i| SampleAccessor {
            sample_table: self,
            index: NonZeroU32::MIN.saturating_add(i),
        })
    }

    /// トラック内のチャンク群の情報を走査するイテレーターを返す
    pub fn chunks(&self) -> impl '_ + Iterator<Item = ChunkAccessor<'_, T>> {
        (0..self.chunk_count()).map(|i| ChunkAccessor {
            sample_table: self,
            index: NonZeroU32::MIN.saturating_add(i),
        })
    }

    /// このインスタンスが保持している [`StblBox`] への参照を返す
    pub fn stbl_box(&self) -> &StblBox {
        self.stbl_box.as_ref()
    }
}

/// [`SampleTableAccessor::new()`] で発生する可能性があるエラー
#[derive(Debug, Clone)]
pub enum SampleTableAccessorError {
    /// [`SttsBox`][crate::boxes::SttsBox] と他のボックスで、表現しているサンプル数が異なる
    InconsistentSampleCount {
        /// [`SttsBox`][crate::boxes::SttsBox] 準拠のサンプル数
        stts_sample_count: u32,

        /// [`SttsBox`][crate::boxes::SttsBox] とは異なるサンプル数を表しているボックスの種別
        other_box_type: BoxType,

        /// `other_box_type` 準拠のサンプル数
        other_sample_count: u32,
    },

    /// [`StscBox`] の最初のエントリのチャンクインデックスが 1 ではない
    FirstChunkIndexIsNotOne {
        /// 実際の最初のチャンクインデックスの値
        actual_chunk_index: NonZeroU32,
    },

    /// [`StscBox`] の最後のエントリのチャンクインデックスが大きすぎる（存在しないチャンクを参照している）
    LastChunkIndexIsTooLarge {
        /// [`StcoBox`][crate::boxes::StcoBox] ないし [`Co64Box`][crate::boxes::Co64Box] が表すチャンクインデックスの最大値
        max_chunk_index: NonZeroU32,

        /// [`StscBox`] の最後のエントリのチャンクインデックス
        last_chunk_index: NonZeroU32,
    },

    /// [`StscBox`] が存在しない [`SampleEntry`] を参照している
    MissingSampleEntry {
        /// [`StscEntry`] のインデックス
        stsc_entry_index: usize,

        /// 存在しないサンプルエントリーのインデックス
        sample_description_index: NonZeroU32,

        /// サンプルエントリーの総数
        sample_entry_count: usize,
    },

    /// [`StscBox`] のチャンクインデックスが短調増加していない
    ChunkIndicesNotMonotonicallyIncreasing,

    /// チャンクは存在するのに stsc エントリーが存在しない
    ChunksExistButNoSamples {
        /// チャンク数
        chunk_count: u32,
    },

    /// サンプル数の累計が [`u32`] の範囲を超えた
    SampleCountOverflow {
        /// オーバーフローが発生したボックスの種別（`stts` ないし `ctts`）
        box_type: BoxType,

        /// オーバーフロー直前までの累計サンプル数
        accumulated_sample_count: u32,

        /// 加算しようとしたエントリのサンプル数
        entry_sample_count: u32,
    },

    /// サンプルデータのバイト位置の累計が [`u64`] の範囲を超えた
    ///
    /// [`Co64Box`][crate::boxes::Co64Box] 由来のチャンクオフセットと
    /// [`StszBox`] 由来のサンプルサイズの累計で発生する。
    /// [`StcoBox`][crate::boxes::StcoBox] はチャンクオフセットが [`u32`] のため
    /// 単独ではオーバーフローしない。
    SampleDataOffsetOverflow {
        /// オフセットの累計がオーバーフローした時点で処理していたサンプルのインデックス
        ///
        /// このサンプル自身の開始位置は正常に算出できており、オーバーフローするのはその終端位置
        /// （同じチャンク内に後続サンプルがあれば、その開始位置になる値）の計算である。
        sample_index: NonZeroU32,

        /// オーバーフロー直前までの累計バイト位置（このサンプルの開始位置）
        accumulated_offset: u64,

        /// 加算しようとしたサンプルのデータサイズ
        sample_data_size: u32,
    },
}

impl core::fmt::Display for SampleTableAccessorError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SampleTableAccessorError::InconsistentSampleCount {
                stts_sample_count,
                other_box_type,
                other_sample_count,
            } => write!(
                f,
                "Sample count in `stts` box is {stts_sample_count}, but `{other_box_type}` has sample count {other_sample_count}"
            ),
            SampleTableAccessorError::FirstChunkIndexIsNotOne { actual_chunk_index } => {
                write!(
                    f,
                    "First chunk index in `stsc` box is expected to be 1, but got {actual_chunk_index}"
                )
            }
            SampleTableAccessorError::LastChunkIndexIsTooLarge {
                max_chunk_index,
                last_chunk_index,
            } => {
                write!(
                    f,
                    "Last chunk index in `stsc` box is expected to be `<= {max_chunk_index}`, but got {last_chunk_index}"
                )
            }
            SampleTableAccessorError::MissingSampleEntry {
                stsc_entry_index,
                sample_description_index,
                sample_entry_count,
            } => {
                write!(
                    f,
                    "{stsc_entry_index}-th entry in `stsc` box refers to a missing sample entry {sample_description_index} (sample entry count is {sample_entry_count})"
                )
            }
            SampleTableAccessorError::ChunkIndicesNotMonotonicallyIncreasing => {
                write!(
                    f,
                    "Chunk indices in `stsc` box are not monotonically increasing"
                )
            }
            SampleTableAccessorError::ChunksExistButNoSamples { chunk_count } => {
                write!(
                    f,
                    "Chunks exist ({chunk_count} chunks) but stsc has no entries"
                )
            }
            SampleTableAccessorError::SampleCountOverflow {
                box_type,
                accumulated_sample_count,
                entry_sample_count,
            } => {
                write!(
                    f,
                    "Total sample count in `{box_type}` box overflows u32 (accumulated {accumulated_sample_count}, adding {entry_sample_count})"
                )
            }
            SampleTableAccessorError::SampleDataOffsetOverflow {
                sample_index,
                accumulated_offset,
                sample_data_size,
            } => {
                write!(
                    f,
                    "Sample data offset overflows u64 at sample {sample_index} (accumulated {accumulated_offset}, adding {sample_data_size})"
                )
            }
        }
    }
}

impl core::error::Error for SampleTableAccessorError {}

/// [`StblBox`] 内の個々のサンプルの情報を取得するための構造体
#[derive(Debug)]
pub struct SampleAccessor<'a, T> {
    sample_table: &'a SampleTableAccessor<T>,
    index: NonZeroU32,
}

impl<'a, T: AsRef<StblBox>> SampleAccessor<'a, T> {
    /// このサンプルのインデックスを取得する
    pub fn index(&self) -> NonZeroU32 {
        self.index
    }

    /// サンプルの尺を取得する
    pub fn duration(&self) -> u32 {
        let i = self
            .sample_table
            .sample_durations
            .binary_search_by_key(&(self.index.get() - 1), |x| x.0)
            .unwrap_or_else(|i| i.checked_sub(1).expect("unreachable"));
        self.sample_table.sample_durations[i].1
    }

    /// サンプルのタイムスタンプ（累計尺）を取得する
    pub fn timestamp(&self) -> u64 {
        let i = self
            .sample_table
            .sample_durations
            .binary_search_by_key(&(self.index.get() - 1), |x| x.0)
            .unwrap_or_else(|i| i.checked_sub(1).expect("unreachable"));
        let (base_index_minus_1, duration, base_timestamp) = self.sample_table.sample_durations[i];
        base_timestamp + duration as u64 * (self.index.get() - 1 - base_index_minus_1) as u64
    }

    /// サンプルのデータサイズ（バイト数）を取得する
    pub fn data_size(&self) -> u32 {
        let i = self.index.get() as usize - 1;
        match &self.sample_table.stbl_box().stsz_box {
            StszBox::Fixed { sample_size, .. } => sample_size.get(),
            StszBox::Variable { entry_sizes } => entry_sizes[i],
        }
    }

    /// サンプルデータのファイル内でのバイト位置を返す
    // `stsz` が `Fixed` の場合は、チャンク先頭オフセットにチャンク内序数 × サンプルサイズを
    // 足して算出する（`new` がオーバーフローを検出済みのため加算は安全）。
    // `stsz` が `Variable` の場合は、`new` が構築した prefix-sum テーブルを参照する。
    pub fn data_offset(&self) -> u64 {
        let idx = self.index.get() - 1;
        match &self.sample_table.stbl_box().stsz_box {
            StszBox::Fixed { sample_size, .. } => {
                let chunk = self.chunk();
                let first_sample_index =
                    self.sample_table.sample_index_offsets[chunk.index().get() as usize - 1];
                chunk.offset()
                    + (self.index.get() - first_sample_index.get()) as u64
                        * sample_size.get() as u64
            }
            StszBox::Variable { .. } => self.sample_table.sample_data_offsets[idx as usize],
        }
    }

    /// サンプルが同期サンプルかどうかを判定する
    pub fn is_sync_sample(&self) -> bool {
        let Some(stss_box) = &self.sample_table.stbl_box().stss_box else {
            // stss ボックスが存在しない場合は全てが同期サンプル扱い
            return true;
        };

        stss_box.sample_numbers.binary_search(&self.index).is_ok()
    }

    /// このサンプルをデコードするために必要となる同期サンプルへの参照を返す
    ///
    /// 自分自身が同期サンプルの場合には、自分が返される。
    /// 自分よりも前方に同期サンプルが存在しない場合には [`None`] が返される。
    pub fn sync_sample(&self) -> Option<Self> {
        let index = if let Some(stss_box) = &self.sample_table.stbl_box().stss_box {
            match stss_box.sample_numbers.binary_search(&self.index) {
                Ok(_) => self.index,
                Err(0) => return None,
                Err(i) => stss_box.sample_numbers[i - 1],
            }
        } else {
            self.index
        };
        Some(Self {
            index,
            sample_table: self.sample_table,
        })
    }

    /// サンプルのコンポジション時間オフセットを取得する
    ///
    /// `ctts` ボックスが存在する場合に、このサンプルに対応するオフセット値を返す。
    /// `ctts` ボックスがない場合は `None` を返す。
    pub fn composition_time_offset(&self) -> Option<i64> {
        let sample_idx = self.index.get() - 1;
        let sample_composition_offsets = self.sample_table.sample_composition_offsets.as_ref()?;
        let i = sample_composition_offsets
            .binary_search_by_key(&sample_idx, |x| x.0)
            .unwrap_or_else(|i| i.checked_sub(1).expect("unreachable"));
        Some(sample_composition_offsets[i].1)
    }

    /// サンプルが属するチャンクの情報を返す
    pub fn chunk(&self) -> ChunkAccessor<'a, T> {
        // sample_per_chunk == 0 のチャンクがあると sample_index_offsets に同一値が連続する。
        // binary_search は重複時の戻りを未規定とするため、index 以下の最右要素を
        // partition_point で明示的に選ぶ（空チャンクを挟んでも実サンプル側のチャンクになる）。
        let i = self
            .sample_table
            .sample_index_offsets
            .partition_point(|x| *x <= self.index)
            .checked_sub(1)
            .expect("valid sample always belongs to a chunk");
        let chunk_index = NonZeroU32::MIN.saturating_add(i as u32);
        self.sample_table
            .get_chunk(chunk_index)
            .expect("unreachable")
    }
}

/// [`StblBox`] 内の個々のチャンクの情報を取得するための構造体
#[derive(Debug)]
pub struct ChunkAccessor<'a, T> {
    sample_table: &'a SampleTableAccessor<T>,
    index: NonZeroU32,
}

impl<'a, T: AsRef<StblBox>> ChunkAccessor<'a, T> {
    /// このチャンクのインデックスを取得する
    pub fn index(&self) -> NonZeroU32 {
        self.index
    }

    /// チャンクのファイル内でのバイト位置を返す
    pub fn offset(&self) -> u64 {
        let i = self.index.get() as usize - 1;
        match &self.sample_table.stbl_box().stco_or_co64_box {
            Either::A(b) => b.chunk_offsets[i] as u64,
            Either::B(b) => b.chunk_offsets[i],
        }
    }

    /// チャンクが参照するサンプルエントリー返す
    pub fn sample_entry(&self) -> &'a SampleEntry {
        &self.sample_table.stbl_box().stsd_box.entries[self.sample_entry_index()]
    }

    /// このチャンクが参照するサンプルエントリーのインデックス（0 ベース）を取得する
    pub fn sample_entry_index(&self) -> usize {
        self.stsc_entry().sample_description_index.get() as usize - 1
    }

    /// チャンクに属するサンプルの数を返す
    pub fn sample_count(&self) -> u32 {
        self.stsc_entry().sample_per_chunk
    }

    /// チャンクに属するサンプル群を走査するイテレーターを返す
    pub fn samples(&self) -> impl '_ + Iterator<Item = SampleAccessor<'_, T>> {
        let count = self.sample_count();
        let sample_index_offset =
            self.sample_table.sample_index_offsets[self.index.get() as usize - 1];
        (0..count).map(move |i| {
            let sample_index = sample_index_offset.saturating_add(i);
            self.sample_table
                .get_sample(sample_index)
                .expect("unreachable")
        })
    }

    fn stsc_entry(&self) -> &StscEntry {
        let i = self
            .sample_table
            .stbl_box()
            .stsc_box
            .entries
            .binary_search_by_key(&self.index, |x| x.first_chunk)
            .unwrap_or_else(|i| i - 1);
        &self.sample_table.stbl_box().stsc_box.entries[i]
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use crate::{
        BaseBox, BoxSize, BoxType,
        boxes::{StcoBox, StscBox, StscEntry, StsdBox, StssBox, SttsBox, UnknownBox},
    };

    use super::*;

    #[test]
    fn sample_table_accessor() {
        let sample_durations = [10, 5, 5, 20, 20, 20, 1, 1, 1, 1];
        let chunk_offsets = [100, 200, 300, 400];
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![SampleEntry::Unknown(UnknownBox {
                    box_type: BoxType::Normal(*b"test"),
                    box_size: BoxSize::U32(8),
                    payload: Vec::new(),
                })],
            },
            stts_box: SttsBox::from_sample_deltas(sample_durations)
                .expect("短い正常系入力で sample_count が溢れることはない"),
            stsc_box: StscBox {
                entries: [(index(1), 2, index(1)), (index(3), 3, index(1))]
                    .into_iter()
                    .map(
                        |(first_chunk, sample_per_chunk, sample_description_index)| StscEntry {
                            first_chunk,
                            sample_per_chunk,
                            sample_description_index,
                        },
                    )
                    .collect(),
            },
            stsz_box: StszBox::Variable {
                entry_sizes: vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: chunk_offsets.to_vec(),
            }),
            stss_box: Some(StssBox {
                sample_numbers: vec![index(1), index(3), index(5), index(7), index(9)],
            }),
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let sample_table = SampleTableAccessor::new(&stbl_box).expect("bug");
        assert_eq!(sample_table.sample_count(), 10);
        assert_eq!(sample_table.chunk_count(), 4);

        let sample_chunks = [1, 1, 2, 2, 3, 3, 3, 4, 4, 4];
        let sample_offsets = [100, 101, 200, 203, 300, 305, 311, 400, 408, 417];
        for i in 0..10 {
            let sample = sample_table.get_sample(index(i as u32 + 1)).expect("bug");
            assert_eq!(sample.duration(), sample_durations[i]);
            assert_eq!(
                sample.timestamp(),
                sample_durations.iter().copied().take(i).sum::<u32>() as u64
            );
            assert_eq!(sample.data_size(), i as u32 + 1);
            assert_eq!(sample.data_offset(), sample_offsets[i] as u64);
            assert_eq!(sample.is_sync_sample(), (i + 1) % 2 == 1);
            assert_eq!(
                sample.sync_sample().map(|s| s.index()),
                Some(NonZeroU32::MIN.saturating_add(i as u32 / 2 * 2))
            );
            assert_eq!(sample.chunk().index().get(), sample_chunks[i]);
        }
        assert!(sample_table.get_sample(index(11)).is_none());

        let sample_counts = [2, 2, 3, 3];
        for i in 0..4 {
            let chunk = sample_table.get_chunk(index(i as u32 + 1)).expect("bug");
            assert_eq!(chunk.offset(), chunk_offsets[i] as u64);
            assert_eq!(chunk.sample_entry().box_type().as_bytes(), b"test");
            assert_eq!(chunk.sample_count(), sample_counts[i]);
            assert_eq!(chunk.samples().count(), sample_counts[i] as usize);
        }
        assert!(sample_table.get_chunk(index(5)).is_none());

        let file_duraiton = sample_durations.iter().copied().sum::<u32>() as u64;
        for t in 0..file_duraiton {
            let index = sample_table.get_sample_by_timestamp(t).expect("bug").index;
            let start_time = sample_table.get_sample(index).expect("bug").timestamp();
            let end_time =
                start_time + sample_table.get_sample(index).expect("bug").duration() as u64;
            assert!((start_time..end_time).contains(&t));
        }
        assert!(
            sample_table
                .get_sample_by_timestamp(file_duraiton + 1)
                .is_none()
        );
    }

    #[test]
    fn sample_table_accessor_empty_stsc_with_chunks_should_error() {
        let stbl_box = StblBox {
            stsd_box: StsdBox {
                entries: vec![SampleEntry::Unknown(UnknownBox {
                    box_type: BoxType::Normal(*b"test"),
                    box_size: BoxSize::U32(8),
                    payload: Vec::new(),
                })],
            },
            stts_box: SttsBox { entries: vec![] },
            stsc_box: StscBox { entries: vec![] }, // 空の stsc
            stsz_box: StszBox::Variable {
                entry_sizes: vec![],
            },
            stco_or_co64_box: Either::A(StcoBox {
                chunk_offsets: vec![100], // 1 つのチャンクオフセット
            }),
            stss_box: None,
            ctts_box: None,
            cslg_box: None,
            sdtp_box: None,
            unknown_boxes: Vec::new(),
        };

        let result = SampleTableAccessor::new(&stbl_box);
        assert!(matches!(
            result,
            Err(SampleTableAccessorError::ChunksExistButNoSamples { chunk_count: 1 })
        ));
    }

    fn index(i: u32) -> NonZeroU32 {
        NonZeroU32::new(i).expect("invalid index")
    }
}
