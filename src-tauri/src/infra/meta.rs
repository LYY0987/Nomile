//! 元数据读取：图片 EXIF、音频 / 视频容器标签。
//!
//! 两条铁律：
//! 1. 只读文件头，不整读大文件 —— 视频几百 MB，不能为了取个标题全读进内存；
//! 2. 失败一律降级为「空元数据 + error 文本」，绝不因为某个文件标签损坏而中断整批重命名。

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use chrono::{Local, NaiveDateTime, TimeZone};

use crate::model::MediaMeta;

/// 走 EXIF 解析的图片格式
const IMAGE_EXTS: &[&str] = &[
    "jpg", "jpeg", "jpe", "jfif", "tif", "tiff", "heic", "heif", "png", "webp", "dng", "cr2",
    "cr3", "nef", "arw", "raf", "orf", "rw2", "sr2", "pef", "3fr", "erf", "kdc",
];

/// 走容器标签解析的音视频格式（lofty 支持的范围）
const MEDIA_EXTS: &[&str] = &[
    "mp3", "m4a", "m4b", "m4v", "mp4", "flac", "ogg", "oga", "opus", "wav", "aif", "aiff", "wv",
    "ape", "aac", "mpc", "wma",
];

/// 这个扩展名能不能读元数据（读不了就不必浪费时间打开文件）
pub fn supports(ext: &str) -> bool {
    let e = ext.to_ascii_lowercase();
    IMAGE_EXTS.contains(&e.as_str()) || MEDIA_EXTS.contains(&e.as_str())
}

pub fn read_meta(path: &Path, ext: &str) -> MediaMeta {
    let e = ext.to_ascii_lowercase();
    if IMAGE_EXTS.contains(&e.as_str()) {
        return read_exif(path);
    }
    if MEDIA_EXTS.contains(&e.as_str()) {
        return read_tags(path);
    }
    MediaMeta::default()
}

// ---------------------------------------------------------------- EXIF

fn read_exif(path: &Path) -> MediaMeta {
    let mut out = MediaMeta::default();

    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) => {
            out.error = Some(format!("打不开文件：{e}"));
            return out;
        }
    };
    let mut reader = BufReader::new(file);

    let exif = match exif::Reader::new().read_from_container(&mut reader) {
        Ok(x) => x,
        Err(e) => {
            out.error = Some(format!("无 EXIF：{e}"));
            return out;
        }
    };

    // 拍摄时间按优先顺序取：原始拍摄 > 数字化时间 > 文件修改时间
    let time_field = exif
        .get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)
        .or_else(|| exif.get_field(exif::Tag::DateTimeDigitized, exif::In::PRIMARY))
        .or_else(|| exif.get_field(exif::Tag::DateTime, exif::In::PRIMARY));

    if let Some(f) = time_field {
        out.shot = ascii_value(&f).and_then(|s| parse_exif_time(&s));
    }

    out.make = str_field(&exif, exif::Tag::Make);
    out.model = str_field(&exif, exif::Tag::Model);
    out.lens = str_field(&exif, exif::Tag::LensModel);
    out.width = uint_field(&exif, exif::Tag::PixelXDimension);
    out.height = uint_field(&exif, exif::Tag::PixelYDimension);
    if out.width.is_none() || out.height.is_none() {
        out.width = out
            .width
            .or_else(|| uint_field(&exif, exif::Tag::ImageWidth));
        out.height = out
            .height
            .or_else(|| uint_field(&exif, exif::Tag::ImageLength));
    }

    if out.error.is_some() && out.has_value() {
        out.error = None;
    }
    out
}

fn ascii_value(f: &exif::Field) -> Option<String> {
    match &f.value {
        exif::Value::Ascii(items) => items.first().and_then(|bytes| {
            let s = String::from_utf8_lossy(bytes);
            let t = s.trim_matches(|c: char| c == '\0' || c.is_whitespace());
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }),
        _ => None,
    }
}

fn str_field(exif: &exif::Exif, tag: exif::Tag) -> Option<String> {
    exif.get_field(tag, exif::In::PRIMARY).and_then(ascii_value)
}

fn uint_field(exif: &exif::Exif, tag: exif::Tag) -> Option<u32> {
    exif.get_field(tag, exif::In::PRIMARY)
        .and_then(|f| f.value.get_uint(0))
}

/// EXIF 时间固定是 `2023:01:01 10:00:00` 这种格式，按本地时区还原成时间戳
fn parse_exif_time(raw: &str) -> Option<i64> {
    let s = raw.trim();
    let naive = NaiveDateTime::parse_from_str(s, "%Y:%m:%d %H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S"))
        .or_else(|_| NaiveDateTime::parse_from_str(s, "%Y:%m:%d %H:%M"))
        .ok()?;
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|dt| dt.timestamp_millis())
}

// ---------------------------------------------------------------- 音视频标签

fn read_tags(path: &Path) -> MediaMeta {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::probe::Probe;
    use lofty::tag::Accessor;

    let mut out = MediaMeta::default();

    let tagged = match Probe::open(path).and_then(|p| p.read()) {
        Ok(t) => t,
        Err(e) => {
            out.error = Some(format!("无可用标签：{e}"));
            return out;
        }
    };

    if let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) {
        out.title = opt_text(tag.title().map(|c| c.to_string()));
        out.artist = opt_text(tag.artist().map(|c| c.to_string()));
        out.album = opt_text(tag.album().map(|c| c.to_string()));
        out.track = tag.track();
    }

    let props = tagged.properties();
    let secs = props.duration().as_secs_f64();
    if secs > 0.0 {
        out.duration = Some(secs);
    }

    out
}

fn opt_text(v: Option<String>) -> Option<String> {
    v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_extension_is_ignored() {
        assert!(!supports("txt"));
        assert!(!supports("pdf"));
        assert!(supports("JPG"));
        assert!(supports("mp4"));
    }

    #[test]
    fn missing_file_degrades_to_error_only() {
        let m = read_meta(Path::new("Z:\\definitely\\not\\here.jpg"), "jpg");
        assert!(m.error.is_some());
        assert!(!m.has_value());
    }

    #[test]
    fn plain_text_file_yields_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.txt");
        std::fs::write(&p, "hello").unwrap();
        let m = read_meta(&p, "txt");
        assert!(!m.has_value());
        assert!(m.error.is_none());
    }

    #[test]
    fn exif_time_is_parsed_as_local() {
        // 2023-01-01 10:00:00 本地时间
        let ts = parse_exif_time("2023:01:01 10:00:00").unwrap();
        let dt = Local.timestamp_millis_opt(ts).unwrap();
        assert_eq!(dt.format("%Y-%m-%d %H:%M:%S").to_string(), "2023-01-01 10:00:00");
    }

    #[test]
    fn exif_time_garbage_returns_none() {
        assert!(parse_exif_time("not a date").is_none());
        assert!(parse_exif_time("").is_none());
    }

    /// 真实 EXIF 解析。需要 NOMILE_TEST_JPEG 指向一张带 DateTimeOriginal 的图，
    /// 没设置就跳过——保证换台机器跑也能过。
    #[test]
    fn reads_real_exif_jpeg() {
        let Ok(path) = std::env::var("NOMILE_TEST_JPEG") else {
            return;
        };
        let m = read_meta(Path::new(&path), "jpg");
        assert!(m.shot.is_some(), "没读到拍摄时间，error={:?}", m.error);
        assert_eq!(m.model.as_deref(), Some("TestCam"));
        assert_eq!(m.make.as_deref(), Some("TestMake"));
    }

    /// 真实音频标签解析。需要 NOMILE_TEST_MP3，没设置就跳过。
    #[test]
    fn reads_real_audio_tags() {
        let Ok(path) = std::env::var("NOMILE_TEST_MP3") else {
            return;
        };
        let m = read_meta(Path::new(&path), "mp3");
        assert_eq!(m.title.as_deref(), Some("TestTitle"), "error={:?}", m.error);
        assert_eq!(m.artist.as_deref(), Some("TestArtist"));
        assert_eq!(m.album.as_deref(), Some("TestAlbum"));
        assert_eq!(m.track, Some(3));
        let d = m.duration.expect("应读到时长");
        assert!((d - 3.0).abs() < 0.5, "时长应约 3 秒，实际 {d}");
    }

    /// 跨层闭环：读到的 EXIF 经模板规则真的变成了文件名
    #[test]
    fn exif_drives_filename() {
        use crate::domain::rules::{apply_stack, RuleContext};
        use crate::model::{FileEntry, Rule, RuleInstance, Target};

        let Ok(path) = std::env::var("NOMILE_TEST_JPEG") else {
            return;
        };
        let meta = read_meta(Path::new(&path), "jpg");

        let entry = FileEntry {
            id: 1,
            dir: "C:\\tmp".into(),
            name: "IMG_0001.jpg".into(),
            stem: "IMG_0001".into(),
            kind: crate::model::FileKind::Image,
            ext: "jpg".into(),
            size: 857,
            modified: 0,
            created: 0,
            is_dir: false,
            readonly: false,
            meta: Some(meta),
        };

        let rules = vec![RuleInstance {
            id: "r1".into(),
            enabled: true,
            target: Target::Name,
            rule: Rule::Template {
                pattern: "${shot}_${camera}.${ext}".into(),
            },
        }];

        let ctx = RuleContext {
            entry: &entry,
            index: 0,
            total: 1,
        };
        let parts = apply_stack(&ctx, &rules).unwrap();
        assert_eq!(
            parts.full(),
            "2023-05-06_070809_TestMake_TestCam.jpg",
            "拍摄时间 + 相机应变成文件名"
        );
    }
}
