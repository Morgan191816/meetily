//! 说话人分离的时间对齐：把 sherpa 片段映射到逐字稿行。

#[derive(Debug, Clone, PartialEq)]
pub struct DiarSegment {
    pub start: f32,
    pub end: f32,
    pub speaker: i32,
}

/// 每行取重叠最大的片段；等量重叠取开始更早者；无重叠且与上一行间隔 < 0.5s 时沿用上一行。
pub fn assign_speakers(
    rows: &[(Option<f64>, Option<f64>)],
    segments: &[DiarSegment],
) -> Vec<Option<usize>> {
    let mut raw: Vec<Option<i32>> = Vec::with_capacity(rows.len());
    let mut previous_end: Option<f64> = None;

    for (start, end) in rows {
        let (Some(start), Some(end)) = (start, end) else {
            raw.push(None);
            previous_end = None;
            continue;
        };
        let (start, end) = (*start, (*end).max(*start));

        let mut best: Option<(f32, f32, i32)> = None; // (overlap, seg_start, speaker)
        for segment in segments {
            let overlap =
                (end.min(segment.end as f64) - start.max(segment.start as f64)).max(0.0) as f32;
            if overlap <= 0.0 {
                continue;
            }
            let better = match best {
                None => true,
                Some((best_overlap, best_start, _)) => {
                    overlap > best_overlap || (overlap == best_overlap && segment.start < best_start)
                }
            };
            if better {
                best = Some((overlap, segment.start, segment.speaker));
            }
        }

        let assigned = match best {
            Some((_, _, speaker)) => Some(speaker),
            None => match (previous_end, raw.last().copied().flatten()) {
                (Some(prev_end), Some(prev_speaker)) if start - prev_end < 0.5 => Some(prev_speaker),
                _ => None,
            },
        };
        raw.push(assigned);
        previous_end = Some(end);
    }

    // 按首次出现顺序重排编号
    let mut order: Vec<i32> = Vec::new();
    for speaker in raw.iter().flatten() {
        if !order.contains(speaker) {
            order.push(*speaker);
        }
    }
    raw.into_iter()
        .map(|speaker| speaker.map(|s| order.iter().position(|o| *o == s).unwrap_or(0)))
        .collect()
}

pub fn label_for(index: usize) -> String {
    format!("SPEAKER_{index:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(start: f32, end: f32, speaker: i32) -> DiarSegment {
        DiarSegment { start, end, speaker }
    }

    #[test]
    fn picks_max_overlap_and_renumbers_by_first_appearance() {
        let rows = vec![(Some(0.0), Some(2.0)), (Some(2.0), Some(4.0)), (Some(4.0), Some(6.0))];
        let segments = [seg(0.0, 2.0, 7), seg(2.0, 4.0, 3), seg(4.0, 6.0, 7)];
        assert_eq!(assign_speakers(&rows, &segments), vec![Some(0), Some(1), Some(0)]);
    }

    #[test]
    fn tie_takes_earlier_segment() {
        let rows = vec![(Some(1.0), Some(3.0))];
        let segments = [seg(0.0, 2.0, 5), seg(2.0, 4.0, 6)];
        assert_eq!(assign_speakers(&rows, &segments), vec![Some(0)]);
    }

    #[test]
    fn inherits_previous_speaker_within_half_second() {
        let rows = vec![(Some(0.0), Some(1.0)), (Some(1.2), Some(2.0))];
        let segments = [seg(0.0, 1.0, 4)];
        assert_eq!(assign_speakers(&rows, &segments), vec![Some(0), Some(0)]);
    }

    #[test]
    fn leaves_gap_longer_than_half_second_unassigned() {
        let rows = vec![(Some(0.0), Some(1.0)), (Some(3.0), Some(4.0))];
        let segments = [seg(0.0, 1.0, 4)];
        assert_eq!(assign_speakers(&rows, &segments), vec![Some(0), None]);
    }

    #[test]
    fn rows_without_times_stay_unassigned() {
        let rows = vec![(None, None), (Some(0.0), Some(1.0))];
        let segments = [seg(0.0, 1.0, 4)];
        assert_eq!(assign_speakers(&rows, &segments), vec![None, Some(0)]);
    }

    #[test]
    fn labels_are_two_digit_zero_padded() {
        assert_eq!(label_for(0), "SPEAKER_00");
        assert_eq!(label_for(12), "SPEAKER_12");
    }
}
