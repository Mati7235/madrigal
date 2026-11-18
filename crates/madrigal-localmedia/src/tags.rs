use std::path::Path;

pub struct EmbeddedArt {
    pub mime: String,
    pub data: Vec<u8>,
}

pub struct FileTags {
    pub title: Option<String>,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub track_number: Option<u32>,
    pub duration_secs: Option<u32>,
    pub artwork: Option<EmbeddedArt>,
    pub lyrics: Option<String>,
}

impl FileTags {
    fn empty() -> Self {
        Self {
            title: None,
            artists: Vec::new(),
            album: None,
            track_number: None,
            duration_secs: None,
            artwork: None,
            lyrics: None,
        }
    }
}

fn err<T>(message: String) -> Result<T, String> {
    Err(message)
}

fn read_u32be(data: &[u8], pos: usize) -> Option<u32> {
    data.get(pos..pos + 4)
        .map(|w| u32::from_be_bytes([w[0], w[1], w[2], w[3]]))
}

fn read_u32le(data: &[u8], pos: usize) -> Option<u32> {
    data.get(pos..pos + 4)
        .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
}

fn read_u64le(data: &[u8], pos: usize) -> Option<u64> {
    data.get(pos..pos + 8).map(|w| {
        u64::from_le_bytes([w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7]])
    })
}

fn decode_latin1(data: &[u8]) -> String {
    data.iter().map(|&b| b as char).collect()
}

fn decode_utf16(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

fn decode_utf16_bytes(data: &[u8], big_endian: bool) -> String {
    let mut units = Vec::with_capacity(data.len() / 2);
    let mut i = 0;
    while i + 1 < data.len() {
        let unit = if big_endian {
            u16::from_be_bytes([data[i], data[i + 1]])
        } else {
            u16::from_le_bytes([data[i], data[i + 1]])
        };
        units.push(unit);
        i += 2;
    }
    decode_utf16(&units)
}

fn decode_text(encoding: u8, data: &[u8]) -> String {
    match encoding {
        1 => {
            if data.starts_with(&[0xFF, 0xFE]) {
                decode_utf16_bytes(&data[2..], false)
            } else if data.starts_with(&[0xFE, 0xFF]) {
                decode_utf16_bytes(&data[2..], true)
            } else {
                decode_utf16_bytes(data, false)
            }
        }
        2 => decode_utf16_bytes(data, true),
        3 => String::from_utf8_lossy(data).into_owned(),
        _ => decode_latin1(data),
    }
}

fn nul_len(encoding: u8, data: &[u8], start: usize) -> Option<usize> {
    if encoding == 1 || encoding == 2 {
        let mut i = start;
        while i + 1 < data.len() {
            if data[i] == 0 && data[i + 1] == 0 && (i - start).is_multiple_of(2) {
                return Some(i + 2 - start);
            }
            i += 1;
        }
        None
    } else {
        data[start..]
            .iter()
            .position(|&b| b == 0)
            .map(|p| p + 1)
    }
}

fn clean_text(raw: &str) -> Option<String> {
    let value = raw.trim_matches(['\0', ' ', '\t', '\r', '\n']).to_owned();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn parse_track_number(raw: &str) -> Option<u32> {
    raw.split('/').next()?.trim().parse::<u32>().ok()
}

fn sniff_mime(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if data.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some("image/png")
    } else if data.starts_with(b"GIF8") {
        Some("image/gif")
    } else if data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP") {
        Some("image/webp")
    } else if data.starts_with(b"BM") {
        Some("image/bmp")
    } else {
        None
    }
}

fn normalize_mime(raw: &str) -> Option<String> {
    let mime = raw.trim().trim_matches('\0').to_lowercase();
    if mime.is_empty() || mime == "-->" {
        return None;
    }
    let mime = mime.replace("jpg", "jpeg");
    if mime.starts_with("image/") {
        Some(mime)
    } else {
        None
    }
}

struct Id3Frames<'a> {
    data: &'a [u8],
    pos: usize,
    end: usize,
    syncsafe: bool,
}

impl<'a> Id3Frames<'a> {
    fn next(&mut self) -> Option<(&'a [u8; 4], &'a [u8])> {
        if self.pos + 10 <= self.end {
            let id: &[u8; 4] = self.data[self.pos..self.pos + 4].try_into().ok()?;
            let raw_size = u32::from_be_bytes(
                self.data[self.pos + 4..self.pos + 8].try_into().ok()?,
            );
            let size = if self.syncsafe {
                unsyncsafe(raw_size)? as usize
            } else {
                raw_size as usize
            };
            self.pos += 10;
            if id == b"\0\0\0\0" {
                return None;
            }
            if !id.iter().all(|b| b.is_ascii_alphanumeric()) {
                return None;
            }
            let body = self.data.get(self.pos..self.pos + size)?;
            self.pos += size;
            return Some((id, body));
        }
        None
    }
}

fn unsyncsafe(raw: u32) -> Option<u32> {
    if raw & 0x80808080 != 0 {
        return None;
    }
    let b0 = (raw >> 24) & 0x7F;
    let b1 = (raw >> 16) & 0x7F;
    let b2 = (raw >> 8) & 0x7F;
    let b3 = raw & 0x7F;
    Some((b0 << 21) | (b1 << 14) | (b2 << 7) | b3)
}

fn id3_tag_span(data: &[u8]) -> Option<(usize, usize, bool)> {
    if data.len() < 10 || &data[0..3] != b"ID3" {
        return None;
    }
    let major = data[3];
    if major != 3 && major != 4 {
        return None;
    }
    let size = unsyncsafe(u32::from_be_bytes([data[6], data[7], data[8], data[9]]))? as usize;
    let end = 10usize.checked_add(size)?;
    if end > data.len() {
        return None;
    }
    let mut start = 10;
    if data[5] & 0x40 != 0 {
        let ext = unsyncsafe(read_u32be(data, 10)?)? as usize;
        start = 10usize.checked_add(ext)?;
        if start > end {
            return None;
        }
    }
    Some((start, end, major == 4))
}

fn apply_text_frame(tags: &mut FileTags, id: &[u8; 4], body: &[u8]) {
    if body.is_empty() {
        return;
    }
    let text = clean_text(&decode_text(body[0], &body[1..]));
    match id {
        b"TIT2" => tags.title = text.or(tags.title.take()),
        b"TPE1" => {
            if let Some(value) = text {
                tags.artists = vec![value];
            }
        }
        b"TALB" => tags.album = text.or(tags.album.take()),
        b"TRCK" => {
            let parsed = text.as_deref().and_then(parse_track_number);
            tags.track_number = tags.track_number.or(parsed);
        }
        _ => {}
    }
}

fn apply_txxx(tags: &mut FileTags, body: &[u8]) {
    if tags.lyrics.is_some() || body.len() < 2 {
        return;
    }
    let encoding = body[0];
    let rest = &body[1..];
    let desc_len = nul_len(encoding, rest, 0).unwrap_or(rest.len());
    let desc = decode_text(encoding, &rest[..desc_len]).to_uppercase();
    let known = desc.trim_matches('\0').trim();
    if known != "USLT" && known != "LYRICS" && !known.starts_with("LYRICS-") {
        return;
    }
    let value = &rest[desc_len.min(rest.len())..];
    let text = clean_text(&decode_text(encoding, value));
    if text.is_some() {
        tags.lyrics = text;
    }
}

fn apply_uslt(tags: &mut FileTags, body: &[u8]) {
    if tags.lyrics.is_some() || body.len() < 5 {
        return;
    }
    let encoding = body[0];
    let rest = &body[4..];
    let skip = nul_len(encoding, rest, 0).unwrap_or(0);
    let text = clean_text(&decode_text(encoding, &rest[skip.min(rest.len())..]));
    if text.is_some() {
        tags.lyrics = text;
    }
}

fn apply_apic(tags: &mut FileTags, body: &[u8]) {
    if tags.artwork.is_some() || body.len() < 5 {
        return;
    }
    let encoding = body[0];
    let rest = &body[1..];
    let mime_len = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
    let mime_raw = &rest[..mime_len];
    let mut pos = mime_len + usize::from(mime_len < rest.len());
    if pos >= rest.len() {
        return;
    }
    pos += 1;
    if pos > rest.len() {
        return;
    }
    let desc_len = nul_len(encoding, rest, pos).unwrap_or(0);
    pos += desc_len;
    if pos > rest.len() {
        return;
    }
    let data = &rest[pos..];
    if data.is_empty() {
        return;
    }
    let mime = normalize_mime(&decode_latin1(mime_raw))
        .or_else(|| sniff_mime(data).map(str::to_owned));
    if let Some(mime) = mime {
        tags.artwork = Some(EmbeddedArt {
            mime,
            data: data.to_vec(),
        });
    }
}

fn parse_id3_frames(tags: &mut FileTags, data: &[u8]) {
    let Some((start, end, syncsafe)) = id3_tag_span(data) else {
        return;
    };
    let mut frames = Id3Frames {
        data,
        pos: start,
        end,
        syncsafe,
    };
    while let Some((id, body)) = frames.next() {
        match id {
            b"TIT2" | b"TPE1" | b"TALB" | b"TRCK" => apply_text_frame(tags, id, body),
            b"USLT" => apply_uslt(tags, body),
            b"TXXX" => apply_txxx(tags, body),
            b"APIC" => apply_apic(tags, body),
            _ => {}
        }
    }
}

fn id3_audio_start(data: &[u8]) -> usize {
    id3_tag_span(data).map(|(_, end, _)| end).unwrap_or(0)
}

const MP3_BR_MPEG1: [u32; 16] = [
    0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0,
];
const MP3_BR_MPEG2: [u32; 16] = [
    0, 8, 16, 24, 32, 40, 48, 56, 64, 80, 96, 112, 128, 144, 160, 0,
];
const MP3_SR: [u32; 4] = [44100, 48000, 32000, 0];

fn mp3_frame(data: &[u8], pos: usize) -> Option<(usize, u32, u32)> {
    if pos + 4 > data.len() || data[pos] != 0xFF || data[pos + 1] & 0xE0 != 0xE0 {
        return None;
    }
    let version = (data[pos + 1] >> 3) & 0x03;
    let layer = (data[pos + 1] >> 1) & 0x03;
    if version == 1 || layer != 1 {
        return None;
    }
    let bitrate_index = (data[pos + 2] >> 4) & 0x0F;
    let rate_index = (data[pos + 2] >> 2) & 0x03;
    let padding = (data[pos + 2] >> 1) & 0x01;
    if bitrate_index == 0 || bitrate_index == 15 || rate_index == 3 {
        return None;
    }
    let mpeg1 = version == 3;
    let bitrate = if mpeg1 {
        MP3_BR_MPEG1[bitrate_index as usize]
    } else {
        MP3_BR_MPEG2[bitrate_index as usize]
    };
    if bitrate == 0 {
        return None;
    }
    let mut rate = MP3_SR[rate_index as usize];
    if !mpeg1 {
        rate /= 2;
        if version == 0 {
            rate /= 2;
        }
    }
    if rate == 0 {
        return None;
    }
    let samples = if mpeg1 { 1152 } else { 576 };
    let mult = if mpeg1 { 144 } else { 72 };
    let len = (mult * bitrate * 1000 / rate + u32::from(padding)) as usize;
    if len < 4 || pos + len > data.len() {
        return None;
    }
    Some((len, samples, rate))
}

fn mp3_duration(data: &[u8], start: usize) -> Option<u32> {
    let mut pos = start;
    let mut samples: u64 = 0;
    let mut rate = 0u32;
    let mut consumed = 0usize;
    let mut frames = 0u32;
    while pos + 4 <= data.len() {
        match mp3_frame(data, pos) {
            Some((len, frame_samples, reported)) => {
                if rate == 0 {
                    rate = reported;
                }
                samples += frame_samples as u64;
                consumed += len;
                pos += len;
                frames += 1;
                if frames >= 20_000 {
                    let rest = data.len().saturating_sub(pos) as u64;
                    if consumed > 0 {
                        samples += rest * samples / consumed as u64;
                    }
                    break;
                }
            }
            None => {
                pos += 1;
            }
        }
    }
    if rate == 0 || samples == 0 {
        return None;
    }
    let secs = samples / rate as u64;
    if secs == 0 || secs > 24 * 3600 {
        None
    } else {
        Some(secs as u32)
    }
}

fn parse_mp3(data: &[u8]) -> Result<FileTags, String> {
    if data.len() < 128 {
        return err(format!("mp3 too small: {} bytes", data.len()));
    }
    if data.len() >= 10 && data[0..3] == *b"ID3" && id3_tag_span(data).is_none() {
        return err("mp3 id3 tag truncated".to_owned());
    }
    let mut tags = FileTags::empty();
    parse_id3_frames(&mut tags, data);
    let start = id3_audio_start(data);
    tags.duration_secs = mp3_duration(data, start);
    if tags.duration_secs.is_none() {
        return err("mp3 has no decodable audio frames".to_owned());
    }
    Ok(tags)
}

fn parse_vorbis_comment(tags: &mut FileTags, data: &[u8], mut pos: usize) -> Option<usize> {
    let vendor_len = read_u32le(data, pos)? as usize;
    pos = pos.checked_add(4 + vendor_len)?;
    let count = read_u32le(data, pos)? as usize;
    pos = pos.checked_add(4)?;
    for _ in 0..count.min(256) {
        let len = read_u32le(data, pos)? as usize;
        pos = pos.checked_add(4)?;
        let entry = data.get(pos..pos.checked_add(len)?)?;
        pos += len;
        let text = String::from_utf8_lossy(entry);
        let (key, value) = match text.split_once('=') {
            Some(pair) => pair,
            None => continue,
        };
        let value = match clean_text(value) {
            Some(value) => value,
            None => continue,
        };
        match key.to_uppercase().as_str() {
            "TITLE" => {
                if tags.title.is_none() {
                    tags.title = Some(value);
                }
            }
            "ARTIST" => tags.artists.push(value),
            "ALBUM" => {
                if tags.album.is_none() {
                    tags.album = Some(value);
                }
            }
            "TRACKNUMBER" | "TRACK" => {
                if tags.track_number.is_none() {
                    tags.track_number = parse_track_number(&value);
                }
            }
            "LYRICS" | "UNSYNCEDLYRICS" => tags.lyrics = tags.lyrics.take().or(Some(value)),
            _ => {}
        }
    }
    Some(pos)
}

fn parse_flac_picture(tags: &mut FileTags, data: &[u8], pos: usize, len: usize) -> Option<()> {
    let mut p = pos;
    let end = pos.checked_add(len)?;
    if end > data.len() {
        return None;
    }
    let _pic_type = read_u32be(data, p)?;
    p += 4;
    let mime_len = read_u32be(data, p)? as usize;
    p += 4;
    let mime = normalize_mime(&decode_latin1(data.get(p..p.checked_add(mime_len)?)?));
    p += mime_len;
    let desc_len = read_u32be(data, p)? as usize;
    p += 4 + desc_len + 16;
    let img_len = read_u32be(data, p)? as usize;
    p += 4;
    let img = data.get(p..p.checked_add(img_len)?)?;
    if p + img_len > end || img.is_empty() {
        return None;
    }
    let mime = mime.or_else(|| sniff_mime(img).map(str::to_owned))?;
    if tags.artwork.is_none() {
        tags.artwork = Some(EmbeddedArt {
            mime,
            data: img.to_vec(),
        });
    }
    Some(())
}

fn parse_flac(data: &[u8]) -> Result<FileTags, String> {
    if data.len() < 42 || &data[0..4] != b"fLaC" {
        return err("not a flac stream".to_owned());
    }
    let mut tags = FileTags::empty();
    let mut pos = 4;
    loop {
        if pos + 4 > data.len() {
            return err("flac metadata truncated".to_owned());
        }
        let last = data[pos] & 0x80 != 0;
        let kind = data[pos] & 0x7F;
        let len = ((data[pos + 1] as usize) << 16)
            | ((data[pos + 2] as usize) << 8)
            | data[pos + 3] as usize;
        pos += 4;
        let end = pos.checked_add(len).ok_or("flac block too large".to_owned())?;
        if end > data.len() {
            return err("flac metadata truncated".to_owned());
        }
        match kind {
            0 => {
                if len < 18 {
                    return err("flac streaminfo too small".to_owned());
                }
                let rate = ((data[pos + 10] as u32) << 12)
                    | ((data[pos + 11] as u32) << 4)
                    | ((data[pos + 12] as u32) >> 4);
                let mut total: u64 = u64::from(data[pos + 13] & 0x0F);
                for b in &data[pos + 14..pos + 18] {
                    total = (total << 8) | u64::from(*b);
                }
                if rate > 0 && total > 0 {
                    tags.duration_secs = Some((total / u64::from(rate)) as u32);
                }
            }
            4 => {
                parse_vorbis_comment(&mut tags, data, pos);
            }
            6 => {
                parse_flac_picture(&mut tags, data, pos, len);
            }
            _ => {}
        }
        pos = end;
        if last {
            break;
        }
    }
    if tags.duration_secs.is_none() {
        return err("flac has no streaminfo duration".to_owned());
    }
    Ok(tags)
}

fn ogg_packets(data: &[u8], serial: u32) -> Result<Vec<Vec<u8>>, String> {
    let mut pos = 0;
    let mut packets = Vec::new();
    let mut current: Vec<u8> = Vec::new();
    while pos + 27 <= data.len() {
        if &data[pos..pos + 4] != b"OggS" {
            return err(format!("ogg sync lost at byte {pos}"));
        }
        let header_type = data[pos + 5];
        let page_serial = read_u32le(data, pos + 14).ok_or("ogg header truncated".to_owned())?;
        let segments = data[pos + 26] as usize;
        if pos + 27 + segments > data.len() {
            return err("ogg segment table truncated".to_owned());
        }
        let table = &data[pos + 27..pos + 27 + segments];
        let mut body_len = 0usize;
        for s in table {
            body_len += *s as usize;
        }
        let body_start = pos + 27 + segments;
        let body_end = body_start + body_len;
        if body_end > data.len() {
            return err("ogg page body truncated".to_owned());
        }
        if page_serial != serial {
            pos = body_end;
            continue;
        }
        if header_type & 0x01 == 0 {
            current.clear();
        }
        current.extend_from_slice(&data[body_start..body_end]);
        if table.last().is_none_or(|&s| s != 255) {
            packets.push(std::mem::take(&mut current));
        }
        pos = body_end;
    }
    Ok(packets)
}

fn ogg_last_granule(data: &[u8], serial: u32) -> Option<u64> {
    let tail = 65536usize.min(data.len());
    let window = &data[data.len() - tail..];
    let mut hit = None;
    let mut i = 0;
    while i + 27 <= window.len() {
        if &window[i..i + 4] == b"OggS"
            && window[i + 4] == 0
            && read_u32le(window, i + 14) == Some(serial)
        {
            hit = Some(i);
        }
        i += 1;
    }
    let i = hit?;
    read_u64le(window, i + 6).filter(|&g| g != u64::MAX)
}

fn parse_ogg(data: &[u8]) -> Result<FileTags, String> {
    if data.len() < 27 + 7 || &data[0..4] != b"OggS" {
        return err("not an ogg stream".to_owned());
    }
    let serial = read_u32le(data, 14).ok_or("ogg header truncated".to_owned())?;
    let packets = ogg_packets(data, serial)?;
    let ident = packets.first().ok_or("ogg has no packets".to_owned())?;
    let mut tags = FileTags::empty();
    if ident.starts_with(b"\x01vorbis") && ident.len() >= 30 {
        let rate = u32::from_le_bytes([ident[12], ident[13], ident[14], ident[15]]);
        if rate == 0 {
            return err("vorbis rate is zero".to_owned());
        }
        let comments = packets.get(1).ok_or("vorbis has no comment packet".to_owned())?;
        if !comments.starts_with(b"\x03vorbis") || comments.len() < 15 {
            return err("vorbis comment packet malformed".to_owned());
        }
        parse_vorbis_comment(&mut tags, comments, 7);
        let granule = ogg_last_granule(data, serial).ok_or("ogg has no granule".to_owned())?;
        tags.duration_secs = Some((granule / u64::from(rate)) as u32);
    } else if ident.starts_with(b"OpusHead") {
        return err("opus in ogg is not supported".to_owned());
    } else {
        return err("unsupported ogg codec".to_owned());
    }
    Ok(tags)
}

fn atom_children(data: &[u8], mut pos: usize, end: usize, out: &mut Vec<([u8; 4], usize, usize)>) {
    while pos + 8 <= end && end <= data.len() {
        let mut size = read_u32be(data, pos).unwrap_or(0) as usize;
        let mut header = 8;
        let name: [u8; 4] = data[pos + 4..pos + 8].try_into().unwrap_or([0; 4]);
        if size == 1 {
            if pos + 16 > end {
                break;
            }
            size = u64::from_be_bytes(data[pos + 8..pos + 16].try_into().unwrap_or([0; 8])) as usize;
            header = 16;
        }
        if size < header || pos + size > end {
            break;
        }
        out.push((name, pos + header, pos + size));
        pos += size;
    }
}

fn find_path(
    data: &[u8],
    root: &[([u8; 4], usize, usize)],
    path: &[&[u8; 4]],
) -> Option<(usize, usize)> {
    let mut level: Vec<([u8; 4], usize, usize)> = root.to_vec();
    for (depth, want) in path.iter().enumerate() {
        let mut next = None;
        for (name, start, end) in &level {
            if name == *want {
                if depth + 1 == path.len() {
                    return Some((*start, *end));
                }
                let mut children = Vec::new();
                let inner = if **want == *b"meta" {
                    start + 4.min(end.saturating_sub(*start))
                } else {
                    *start
                };
                atom_children(data, inner, *end, &mut children);
                next = Some(children);
                break;
            }
        }
        level = next?;
    }
    None
}

fn mp4_text_value(data: &[u8], start: usize, end: usize) -> Option<String> {
    let pos = start;
    if pos + 8 <= end {
        let size = read_u32be(data, pos)? as usize;
        if &data.get(pos + 4..pos + 8)? != b"data" || size < 16 || pos + size > end {
            return None;
        }
        let value = data.get(pos + 16..pos + size)?;
        return clean_text(&String::from_utf8_lossy(value));
    }
    None
}

fn mp4_track_number(data: &[u8], start: usize, end: usize) -> Option<u32> {
    let pos = start;
    if pos + 8 <= end {
        let size = read_u32be(data, pos)? as usize;
        if &data.get(pos + 4..pos + 8)? != b"data" || size < 16 || pos + size > end {
            return None;
        }
        let value = data.get(pos + 16..pos + size)?;
        let num = u16::from_be_bytes(value.get(2..4)?.try_into().ok()?) as u32;
        return if num == 0 { None } else { Some(num) };
    }
    None
}

fn mp4_cover(data: &[u8], start: usize, end: usize) -> Option<EmbeddedArt> {
    let pos = start;
    if pos + 8 <= end {
        let size = read_u32be(data, pos)? as usize;
        if &data.get(pos + 4..pos + 8)? != b"data" || size < 16 || pos + size > end {
            return None;
        }
        let kind = read_u32be(data, pos + 8)?;
        let bytes = data.get(pos + 16..pos + size)?.to_vec();
        if bytes.is_empty() {
            return None;
        }
        let mime = match kind {
            13 => Some("image/jpeg".to_owned()),
            14 => Some("image/png".to_owned()),
            27 => Some("image/bmp".to_owned()),
            _ => sniff_mime(&bytes).map(str::to_owned),
        }?;
        return Some(EmbeddedArt { mime, data: bytes });
    }
    None
}

fn mp4_mvhd_duration(data: &[u8], start: usize, end: usize) -> Option<u32> {
    let mut children = Vec::new();
    atom_children(data, start, end, &mut children);
    for (name, s, e) in &children {
        if name == b"mvhd" && e - s >= 20 {
            let version = data[*s];
            let (scale, len) = if version == 1 {
                (read_u32be(data, s + 20)?, read_u64le(data, s + 24)?)
            } else {
                (read_u32be(data, s + 12)?, u64::from(read_u32be(data, s + 16)?))
            };
            if scale == 0 {
                return None;
            }
            return Some((len / u64::from(scale)) as u32);
        }
    }
    None
}

fn parse_mp4(data: &[u8]) -> Result<FileTags, String> {
    if data.len() < 12 {
        return err("mp4 too small".to_owned());
    }
    let mut top = Vec::new();
    atom_children(data, 0, data.len(), &mut top);
    let moov = top
        .iter()
        .find(|(name, _, _)| name == b"moov")
        .map(|(_, s, e)| (*s, *e))
        .ok_or("mp4 has no moov".to_owned())?;
    let mut tags = FileTags::empty();
    tags.duration_secs = mp4_mvhd_duration(data, moov.0, moov.1);
    let mut moov_children = Vec::new();
    atom_children(data, moov.0, moov.1, &mut moov_children);
    let found = find_path(data, &moov_children, &[b"udta", b"meta", b"ilst"]);
    if let Some((start, end)) = found {
        let mut items = Vec::new();
        atom_children(data, start, end, &mut items);
        for (name, s, e) in &items {
            match name {
                b"\xa9nam" => tags.title = mp4_text_value(data, *s, *e).or(tags.title.take()),
                b"\xa9ART" => {
                    if let Some(value) = mp4_text_value(data, *s, *e) {
                        tags.artists = vec![value];
                    }
                }
                b"\xa9alb" => tags.album = mp4_text_value(data, *s, *e).or(tags.album.take()),
                b"\xa9lyr" => tags.lyrics = mp4_text_value(data, *s, *e).or(tags.lyrics.take()),
                b"trkn" => {
                    tags.track_number = tags.track_number.or(mp4_track_number(data, *s, *e));
                }
                b"covr" => tags.artwork = tags.artwork.take().or(mp4_cover(data, *s, *e)),
                _ => {}
            }
        }
    }
    if tags.duration_secs.is_none() {
        return err("mp4 has no mvhd duration".to_owned());
    }
    Ok(tags)
}

fn parse_wav(data: &[u8]) -> Result<FileTags, String> {
    if data.len() < 12 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return err("not a wav stream".to_owned());
    }
    let mut tags = FileTags::empty();
    let mut pos = 12;
    let mut byte_rate = 0u32;
    let mut data_len = 0u64;
    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let len = read_u32le(data, pos + 4).unwrap_or(0) as usize;
        let body = pos + 8;
        let end = body.saturating_add(len).min(data.len());
        match id {
            b"fmt " => {
                if end - body >= 12 {
                    byte_rate = read_u32le(data, body + 8).unwrap_or(0);
                }
            }
            b"data" => data_len = (end - body) as u64,
            b"LIST" => {
                if data.get(body..body + 4) == Some(b"INFO") {
                    let mut p = body + 4;
                    while p + 8 <= end {
                        let sub = &data[p..p + 4];
                        let slen = read_u32le(data, p + 4).unwrap_or(0) as usize;
                        let sbody = p + 8;
                        let send = sbody.saturating_add(slen).min(end);
                        let raw = data.get(sbody..send).unwrap_or(&[]);
                        let text = clean_text(&decode_latin1(raw));
                        match sub {
                            b"INAM" => tags.title = text.or(tags.title.take()),
                            b"IART" => {
                                if let Some(value) = text {
                                    tags.artists = vec![value];
                                }
                            }
                            b"IPRD" => tags.album = text.or(tags.album.take()),
                            b"IPRT" => {
                                let parsed = text.as_deref().and_then(parse_track_number);
                                tags.track_number = tags.track_number.or(parsed);
                            }
                            _ => {}
                        }
                        p = send + (slen & 1);
                    }
                }
            }
            b"id3 " => {
                let mut id3_tags = FileTags::empty();
                parse_id3_frames(&mut id3_tags, &data[body..end]);
                if tags.title.is_none() {
                    tags.title = id3_tags.title;
                }
                if tags.artists.is_empty() {
                    tags.artists = id3_tags.artists;
                }
                if tags.album.is_none() {
                    tags.album = id3_tags.album;
                }
                if tags.track_number.is_none() {
                    tags.track_number = id3_tags.track_number;
                }
                if tags.lyrics.is_none() {
                    tags.lyrics = id3_tags.lyrics;
                }
                if tags.artwork.is_none() {
                    tags.artwork = id3_tags.artwork;
                }
            }
            _ => {}
        }
        pos = end + (len & 1);
    }
    if byte_rate == 0 || data_len == 0 {
        return err("wav has no audio data".to_owned());
    }
    tags.duration_secs = Some((data_len / u64::from(byte_rate)) as u32);
    Ok(tags)
}

pub fn parse_file(path: &Path) -> Result<FileTags, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let data =
        std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    match ext.as_str() {
        "mp3" => parse_mp3(&data),
        "flac" => parse_flac(&data),
        "ogg" | "oga" => parse_ogg(&data),
        "m4a" | "mp4" => parse_mp4(&data),
        "wav" => parse_wav(&data),
        _ => err(format!("unsupported extension: {ext}")),
    }
}
