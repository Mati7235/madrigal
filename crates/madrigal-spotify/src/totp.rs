pub fn base32_decode(input: &str) -> Vec<u8> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
    let mut out = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits_left: u32 = 0;
    for byte in input.bytes().map(|b| b.to_ascii_uppercase()) {
        if byte == b'=' {
            continue;
        }
        let value = match ALPHABET.iter().position(|slot| *slot == byte) {
            Some(index) => index as u32,
            None => continue,
        };
        buffer = (buffer << 5) | value;
        bits_left += 5;
        if bits_left >= 8 {
            bits_left -= 8;
            out.push(((buffer >> bits_left) & 0xFF) as u8);
        }
    }
    out
}

fn sha1_block(block: &[u8; 64], state: &mut [u32; 5]) {
    let mut words = [0u32; 80];
    for index in 0..16 {
        words[index] = u32::from_be_bytes([
            block[index * 4],
            block[index * 4 + 1],
            block[index * 4 + 2],
            block[index * 4 + 3],
        ]);
    }
    for index in 16..80 {
        words[index] = (words[index - 3] ^ words[index - 8] ^ words[index - 14] ^ words[index - 16])
            .rotate_left(1);
    }
    let (mut a, mut b, mut c, mut d, mut e) =
        (state[0], state[1], state[2], state[3], state[4]);
    for (index, word) in words.iter().enumerate() {
        let (f, k) = match index {
            0..20 => ((b & c) | ((!b) & d), 0x5A827999),
            20..40 => (b ^ c ^ d, 0x6ED9EBA1),
            40..60 => ((b & c) | (b & d) | (c & d), 0x8F1BBCDC),
            _ => (b ^ c ^ d, 0xCA62C1D6),
        };
        let next = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(*word);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = next;
    }
    state[0] = state[0].wrapping_add(a);
    state[1] = state[1].wrapping_add(b);
    state[2] = state[2].wrapping_add(c);
    state[3] = state[3].wrapping_add(d);
    state[4] = state[4].wrapping_add(e);
}

pub fn sha1(message: &[u8]) -> [u8; 20] {
    let mut state = [0x67452301u32, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0];
    let mut length = message.len() as u64;
    let mut cursor = 0;
    while cursor + 64 <= message.len() {
        let mut block = [0u8; 64];
        block.copy_from_slice(&message[cursor..cursor + 64]);
        sha1_block(&block, &mut state);
        cursor += 64;
    }
    let mut tail = Vec::from(&message[cursor..]);
    tail.push(0x80);
    while tail.len() % 64 != 56 {
        tail.push(0);
    }
    length *= 8;
    tail.extend_from_slice(&length.to_be_bytes());
    let (head, _) = tail.as_chunks::<64>();
    for chunk in head {
        let mut block = [0u8; 64];
        block.copy_from_slice(chunk);
        sha1_block(&block, &mut state);
    }
    let mut digest = [0u8; 20];
    for (index, word) in state.iter().enumerate() {
        digest[index * 4..index * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

pub fn hmac_sha1(key: &[u8], message: &[u8]) -> [u8; 20] {
    let mut padded = [0u8; 64];
    if key.len() > 64 {
        let digest = sha1(key);
        padded[..20].copy_from_slice(&digest);
    } else {
        padded[..key.len()].copy_from_slice(key);
    }
    let inner: Vec<u8> = padded
        .iter()
        .map(|byte| byte ^ 0x36)
        .chain(message.iter().copied())
        .collect();
    let inner_digest = sha1(&inner);
    let outer: Vec<u8> = padded
        .iter()
        .map(|byte| byte ^ 0x5C)
        .chain(inner_digest.iter().copied())
        .collect();
    sha1(&outer)
}

pub fn generate(secret: &str, server_time_secs: i64) -> String {
    let key = base32_decode(secret);
    let step = (server_time_secs.max(0) as u64) / 30;
    let mut counter = [0u8; 8];
    for (index, slot) in counter.iter_mut().enumerate() {
        *slot = ((step >> (8 * (7 - index))) & 0xFF) as u8;
    }
    let hash = hmac_sha1(&key, &counter);
    let offset = (hash[19] & 0x0F) as usize;
    let code = ((u32::from(hash[offset] & 0x7F)) << 24)
        | (u32::from(hash[offset + 1]) << 16)
        | (u32::from(hash[offset + 2]) << 8)
        | u32::from(hash[offset + 3]);
    format!("{:06}", code % 1_000_000)
}

pub fn valid_secret(secret: &str) -> bool {
    !secret.is_empty()
        && secret.bytes().all(|byte| {
            byte == b'='
                || matches!(
                    byte.to_ascii_uppercase(),
                    b'A'..=b'Z' | b'2'..=b'7'
                )
        })
}
