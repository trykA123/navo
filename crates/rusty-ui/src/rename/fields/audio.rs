use std::path::Path;

use lofty::config::ParseOptions;
use lofty::file::{TaggedFile, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Accessor;
use rusty_core::rename::{FieldInfo, FieldProvider, Values};

static FIELDS: [FieldInfo; 6] = [
    FieldInfo::builtin("artist", "rename-field-audio-artist"),
    FieldInfo::builtin("title", "rename-field-audio-title"),
    FieldInfo::builtin("album", "rename-field-audio-album"),
    FieldInfo::builtin("track", "rename-field-audio-track"),
    FieldInfo::builtin("year", "rename-field-audio-year"),
    FieldInfo::builtin("genre", "rename-field-audio-genre"),
];

pub struct AudioFields;

impl FieldProvider for AudioFields {
    fn group(&self) -> &str {
        "audio"
    }

    fn fields(&self) -> &[FieldInfo] {
        &FIELDS
    }

    fn read(&self, path: &Path) -> Values {
        let mut values = Values::default();
        let Some(file) = open(path) else {
            return values;
        };
        let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) else {
            return values;
        };
        let found = [
            ("artist", tag.artist()),
            ("title", tag.title()),
            ("album", tag.album()),
            (
                "track",
                tag.track().map(|track| format!("{track:02}").into()),
            ),
            ("year", tag.date().map(|date| date.year.to_string().into())),
            ("genre", tag.genre()),
        ];
        for (field, value) in found {
            if let Some(value) = value {
                values.insert(field, &value);
            }
        }
        values
    }
}

fn open(path: &Path) -> Option<TaggedFile> {
    Probe::open(path)
        .ok()?
        .options(
            ParseOptions::new()
                .read_properties(false)
                .read_cover_art(false),
        )
        .guess_file_type()
        .ok()?
        .read()
        .ok()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use lofty::config::WriteOptions;
    use lofty::tag::items::Timestamp;
    use lofty::tag::{Accessor, Tag, TagExt, TagType};
    use rusty_core::rename::Values;

    use super::super::builtin;

    const ALL: [&str; 6] = ["artist", "title", "album", "track", "year", "genre"];

    fn empty_mp3() -> Vec<u8> {
        let mut bytes = vec![0; 834];
        for frame in bytes.as_chunks_mut::<417>().0 {
            frame[..4].copy_from_slice(&[0xff, 0xfb, 0x90, 0]);
        }
        bytes
    }

    fn empty_flac() -> Vec<u8> {
        let mut bytes = b"fLaC\x80\0\0\x22".to_vec();
        let mut stream_info = [0; 34];
        stream_info[..4].copy_from_slice(&[0, 16, 0, 16]);
        let properties = (44100_u64 << 44) | (1 << 41) | (15 << 36);
        stream_info[10..18].copy_from_slice(&properties.to_be_bytes());
        bytes.extend(stream_info);
        bytes
    }

    fn write_tag(path: &Path, tag: &Tag) {
        let bytes = if tag.tag_type() == TagType::Id3v2 {
            empty_mp3()
        } else {
            empty_flac()
        };
        std::fs::write(path, bytes).unwrap();
        tag.save_to_path(path, WriteOptions::default()).unwrap();
    }

    fn read(path: &Path) -> Values {
        builtin().provider("audio").unwrap().read(path)
    }

    fn full_tag(tag_type: TagType) -> Tag {
        let mut tag = Tag::new(tag_type);
        tag.set_artist("Test Ensemble".into());
        tag.set_title("Dawn/Dusk: Part I".into());
        tag.set_album("Paper Moons".into());
        tag.set_track(7);
        tag.set_track_total(12);
        tag.set_date(Timestamp {
            year: 2026,
            month: Some(10),
            day: Some(20),
            ..Timestamp::default()
        });
        tag.set_genre("Synthetic".into());
        tag
    }

    fn assert_fields(values: &Values) {
        for (field, expected) in ALL.into_iter().zip([
            "Test Ensemble",
            "Dawn_Dusk_ Part I",
            "Paper Moons",
            "07",
            "2026",
            "Synthetic",
        ]) {
            assert_eq!(values.get(field), Some(expected), "{field}");
        }
        super::super::tests::assert_all_safe(values, &ALL);
    }

    #[test]
    fn audio_fields_have_the_expected_labels() {
        let registry = builtin();
        let provider = registry.provider("audio").unwrap();
        for (field, name) in provider.fields().iter().zip(ALL) {
            assert_eq!(field.name, name);
            assert_eq!(field.label_id, format!("rename-field-audio-{name}"));
        }
        assert_eq!(provider.fields().len(), ALL.len());
    }

    #[test]
    fn id3v2_fields_are_read_from_an_mp3_and_made_name_safe() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.mp3");
        write_tag(&path, &full_tag(TagType::Id3v2));
        assert_fields(&read(&path));
    }

    #[test]
    fn vorbis_fields_are_read_from_a_flac_and_made_name_safe() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sample.flac");
        write_tag(&path, &full_tag(TagType::VorbisComments));
        assert_fields(&read(&path));
    }

    #[test]
    fn track_is_a_number_padded_to_at_least_two_digits_without_the_total() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("track.mp3");
        for (track, expected) in [(0, "00"), (1, "01"), (9, "09"), (12, "12"), (123, "123")] {
            let mut tag = Tag::new(TagType::Id3v2);
            tag.set_track(track);
            tag.set_track_total(200);
            write_tag(&path, &tag);
            assert_eq!(read(&path).get("track"), Some(expected));
        }
    }

    #[test]
    fn missing_fields_give_no_value() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.mp3");
        let mut tag = Tag::new(TagType::Id3v2);
        tag.set_title("Only a Title".into());
        write_tag(&path, &tag);
        let mut expected = Values::default();
        expected.insert("title", "Only a Title");
        assert_eq!(read(&path), expected);
    }

    #[test]
    fn audio_without_tags_gives_no_values() {
        let dir = tempfile::tempdir().unwrap();
        for (name, bytes) in [("plain.mp3", empty_mp3()), ("plain.flac", empty_flac())] {
            let path = dir.path().join(name);
            std::fs::write(&path, bytes).unwrap();
            assert_eq!(read(&path), Values::default());
        }
    }

    #[test]
    fn non_audio_invalid_and_unreadable_files_give_no_values() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["notes.txt", "invalid.mp3", "invalid.flac"] {
            let path = dir.path().join(name);
            std::fs::write(&path, b"invented plain text").unwrap();
            assert_eq!(read(&path), Values::default());
        }
        assert_eq!(read(&dir.path().join("missing.mp3")), Values::default());
        assert_eq!(read(dir.path()), Values::default());
    }
}
