// Copyright 2026 Quantova Inc
// SPDX-License-Identifier: Apache-2.0 OR MIT

const MANIFEST: &str = include_str!("../Cargo.toml");

#[allow(dead_code)]
pub fn short_rev(dependency: &str) -> String {
    for line in MANIFEST.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix(dependency) else {
            continue;
        };
        if !rest.trim_start().starts_with('=') {
            continue;
        }
        if let Some(at) = rest.find("rev = \"") {
            let tail = &rest[at + "rev = \"".len()..];
            if let Some(end) = tail.find('"') {
                let rev = &tail[..end];
                return rev.chars().take(7).collect();
            }
        }
    }
    "unpinned".to_string()
}

#[allow(dead_code)]
pub fn stack_line() -> String {
    format!(
        "Quantova-Chain {}, QVM {}, QRC-CONSENSUS {}, Q-Crypto {}",
        short_rev("qtv-codec"),
        short_rev("qtv-vm"),
        short_rev("qtv-attest"),
        short_rev("qtv-crypto"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pin_is_read_from_the_manifest_and_not_from_prose() {
        let crypto = short_rev("qtv-crypto");
        assert_eq!(
            crypto.len(),
            7,
            "a revision is reported short, got {crypto}"
        );
        assert!(
            MANIFEST.contains(&crypto),
            "the reported revision must appear in the manifest it came from"
        );
        assert_ne!(
            crypto, "33c7e93",
            "the hand typed value the banner used to carry is not the pin"
        );
        assert_eq!(
            short_rev("qtv-not-a-dependency"),
            "unpinned",
            "a dependency that is not pinned says so rather than inventing one"
        );
    }

    #[test]
    fn the_stack_line_names_every_part() {
        let line = stack_line();
        for part in ["Quantova-Chain", "QVM", "QRC-CONSENSUS", "Q-Crypto"] {
            assert!(line.contains(part), "{part} is missing from {line}");
        }
        assert!(
            !line.contains("unpinned"),
            "every part of the stack must resolve to a real pin, got {line}"
        );
    }
}
