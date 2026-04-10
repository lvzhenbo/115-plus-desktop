use super::types::{MIN_SEGMENT_SIZE, Segment, SegmentStatus};

/// 计算下载分片范围。
///
/// - file_size == 0：返回空 Vec
/// - file_size < MIN_SEGMENT_SIZE：退化为单分片
/// - 其他情况：自适应裁剪分片数，等分后由最后一个分片吸收余数
pub fn compute_segments(file_size: u64, configured_count: u16) -> Vec<Segment> {
    if file_size == 0 {
        return Vec::new();
    }

    if file_size < MIN_SEGMENT_SIZE {
        return vec![Segment {
            index: 0,
            start: 0,
            end: file_size - 1,
            status: SegmentStatus::Pending,
            downloaded: 0,
        }];
    }

    // 根据文件大小裁剪分片数，避免出现大量小于 1MB 的无效分片。
    let max_by_size = (file_size / MIN_SEGMENT_SIZE) as u16;
    let actual_count = configured_count.min(max_by_size).max(1);

    let base_size = file_size / actual_count as u64;
    let remainder = file_size % actual_count as u64;

    let mut segments = Vec::with_capacity(actual_count as usize);
    let mut offset: u64 = 0;

    for i in 0..actual_count {
        let size = if i == actual_count - 1 {
            base_size + remainder
        } else {
            base_size
        };
        segments.push(Segment {
            index: i,
            start: offset,
            end: offset + size - 1,
            status: SegmentStatus::Pending,
            downloaded: 0,
        });
        offset += size;
    }

    segments
}
