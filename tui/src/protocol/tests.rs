use super::*;

#[test]
fn protocol_vectors_at_every_split() {
    use KeyCode::*;
    use KeyKind::*;

    // wire examples from kitty's keyboard protocol and xterm's control sequences
    let none = Modifiers::default();
    let control = Modifiers {
        control: true,
        ..none
    };
    let shift = Modifiers {
        shift: true,
        ..none
    };
    let plain = Key {
        code: Character('a'),
        shifted: None,
        modifiers: Modifiers::default(),
        kind: Press,
        text: false,
    };
    let fixtures: &[(&[u8], &[Event])] = &[
        (b"\x1b[57414u", &[key(Enter, none, Press)]),
        (b"\x1b[57419u", &[key(Up, none, Press)]),
        (b"\x1b[57366u", &[key(Function(3), none, Press)]),
        (b"\x1b[57359u", &[key(Unknown, none, Press)]),
        (
            b"\x1b[<35;1;1M",
            &[Event::Mouse {
                kind: MouseKind::Move,
                modifiers: Modifiers::default(),
                column: 0,
                row: 0,
            }],
        ),
        (
            b"\x1b[<84;1;1M",
            &[Event::Mouse {
                kind: MouseKind::Scroll { x: 0, y: -1 },
                modifiers: Modifiers {
                    shift: true,
                    control: true,
                    ..Modifiers::default()
                },
                column: 0,
                row: 0,
            }],
        ),
        (
            b"\x1b[<67;1;1M",
            &[Event::Mouse {
                kind: MouseKind::Scroll { x: 1, y: 0 },
                modifiers: Modifiers::default(),
                column: 0,
                row: 0,
            }],
        ),
        (
            b"\x1b[<129;1;1m",
            &[Event::Mouse {
                kind: MouseKind::Up(MouseButton::Forward),
                modifiers: Modifiers::default(),
                column: 0,
                row: 0,
            }],
        ),
        (b"\xc2\r\xa9", &[key(Enter, none, Press)]),
        (b"\x1b[13~", &[key(Function(3), none, Press)]),
        (
            b"\x1b[7~\x1b[8~",
            &[key(Home, none, Press), key(End, none, Press)],
        ),
        (b"\x1b[97;1:3u", &[key(Character('a'), none, Release)]),
        (
            b"\x1b[97:65:97;2;65u",
            &[
                Event::Key(Key {
                    shifted: Some('A'),
                    modifiers: shift,
                    text: true,
                    ..plain
                }),
                Event::Text('A'),
            ],
        ),
        (
            b"\x1b[128104;1;128104:8205:128105:8205:128103u",
            &[
                Event::Key(Key {
                    code: Character('👨'),
                    text: true,
                    ..plain
                }),
                Event::Text('👨'),
                Event::Text('\u{200d}'),
                Event::Text('👩'),
                Event::Text('\u{200d}'),
                Event::Text('👧'),
            ],
        ),
        (
            b"\x04\x01\x02\x03\x05\x1b[<0;10;20M",
            &[Event::Mouse {
                kind: MouseKind::Down(MouseButton::Left),
                modifiers: Modifiers::default(),
                column: 9,
                row: 19,
            }],
        ),
        (
            "a😀é".as_bytes(),
            &[Event::Text('a'), Event::Text('😀'), Event::Text('é')],
        ),
        (b"\x1b[27u", &[key(Escape, none, Press)]),
        (b"\x1b[128512u", &[key(Character('😀'), none, Press)]),
        (b"\x1b[97;5u", &[key(Character('a'), control, Press)]),
        (b"\x1b[97;5:2u", &[key(Character('a'), control, Repeat)]),
        (b"\x1b[97;5:3u", &[key(Character('a'), control, Release)]),
        (
            b"\x1b[97:65:113;2u",
            &[Event::Key(Key {
                shifted: Some('A'),
                modifiers: shift,
                ..plain
            })],
        ),
        (
            b"\x1b[97;256u",
            &[Event::Key(Key {
                modifiers: Modifiers {
                    shift: true,
                    control: true,
                    alt: true,
                    super_key: true,
                },
                ..plain
            })],
        ),
        (
            b"\x1b[121::122;;121u",
            &[
                Event::Key(Key {
                    code: Character('y'),
                    text: true,
                    ..plain
                }),
                Event::Text('y'),
            ],
        ),
        (
            b"\x1b[97;;97:769u",
            &[
                Event::Key(Key {
                    text: true,
                    ..plain
                }),
                Event::Text('a'),
                Event::Text('\u{301}'),
            ],
        ),
        (
            b"\x1b[A\x1b[1;5:2D",
            &[key(Up, none, Press), key(Left, control, Repeat)],
        ),
        (
            b"\x1b[15~\x1b[24~",
            &[
                key(Function(5), none, Press),
                key(Function(12), none, Press),
            ],
        ),
        (
            b"\x1b[<128;12;3M",
            &[Event::Mouse {
                kind: MouseKind::Down(MouseButton::Back),
                modifiers: Modifiers::default(),
                column: 11,
                row: 2,
            }],
        ),
        (
            b"\x1b[<0;12;3m",
            &[Event::Mouse {
                kind: MouseKind::Up(MouseButton::Left),
                modifiers: Modifiers::default(),
                column: 11,
                row: 2,
            }],
        ),
        (
            b"\x1b[I\x1b[O\x1b[?997;1n\x1b[?997;2n",
            &[
                Event::Focus(true),
                Event::Focus(false),
                Event::Theme(true),
                Event::Theme(false),
            ],
        ),
        (
            b"\x1b]10;rgb:ffff/8888/0000\x07",
            &[Event::Color {
                slot: PaletteSlot::Foreground,
                rgb: [255, 136, 0],
            }],
        ),
        (
            b"\x1b]11;rgb:1/2/3\x1b\\",
            &[Event::Color {
                slot: PaletteSlot::Background,
                rgb: [17, 34, 51],
            }],
        ),
        (
            b"\x1b]4;3;rgb:ff/88/00;255;rgb:111/222/333\x1b\\",
            &[
                Event::Color {
                    slot: PaletteSlot::Indexed(3),
                    rgb: [255, 136, 0],
                },
                Event::Color {
                    slot: PaletteSlot::Indexed(255),
                    rgb: [17, 34, 51],
                },
            ],
        ),
    ];
    for &(bytes, expected) in fixtures {
        for split in 0..=bytes.len() {
            let mut parser = Parser::default();
            let mut events = Vec::new();
            parser.parse(&bytes[..split], |event| events.push(event));
            parser.parse(&bytes[split..], |event| events.push(event));
            assert_eq!(events, expected, "{bytes:?} split at {split}");
        }
    }
    let mut parser = Parser::default();
    let mut events = Vec::new();
    for &(bytes, _) in fixtures {
        for byte in bytes {
            parser.parse(&[*byte], |event| events.push(event));
        }
    }
    assert_eq!(
        events,
        fixtures
            .iter()
            .flat_map(|(_, events)| events.iter().copied())
            .collect::<Vec<_>>()
    );
}

#[test]
fn paste_is_literal_and_streamed() {
    let content = "hello\n😀\x1b[27u\x1b]10;rgb:f/f/f\x07\x1b[201x\x1b";
    let input = format!("\x1b[200~{content}\x1b[201~z");
    for split in 0..=input.len() {
        let mut parser = Parser::default();
        let mut actual = String::new();
        let mut emit = |event| match event {
            Event::Text(c) => actual.push(c),
            _ => panic!("paste became a command"),
        };
        parser.parse(&input.as_bytes()[..split], &mut emit);
        parser.parse(&input.as_bytes()[split..], &mut emit);
        assert_eq!(actual, format!("{content}z"));
    }
}

#[test]
fn malformed_and_oversized_sequences_recover() {
    for input in [
        "\x1b[4294967296u",
        "\x1b[97;0u",
        "\x1b[97;257u",
        "\x1b[97;5:9u",
        "\x1b[97;;1114112u",
        "\x1b[<0;0;2M",
        "\x1b[<0;1;2;3M",
        "\x1b[?1u",
        "\x1b[1;2R",
        "\x1b]10;rgb:ffff/ffff/ffff/ffff\x07",
        "\x1b]4;256;rgb:f/f/f\x07",
        "\x1b]11;rgb:é/0/0\x1b\\",
        "\x1bPignored\x1b\\",
        "\x1b_unknown\x1b\\",
    ] {
        let mut parser = Parser::default();
        let mut events = Vec::new();
        parser.parse(input.as_bytes(), |e| events.push(e));
        parser.parse(b"x", |e| events.push(e));
        assert_eq!(events, [Event::Text('x')], "{input:?}");
    }
    let mut parser = Parser::default();
    let mut events = Vec::new();
    parser.parse(b"\x1b[", |e| events.push(e));
    for _ in 0..5000 {
        parser.parse(b"1", |e| events.push(e));
    }
    parser.parse(b"ux\x1b]10;", |e| events.push(e));
    for _ in 0..5000 {
        parser.parse(b"a", |e| events.push(e));
    }
    parser.parse(b"\x07y\x1b]broken\x1b[27u", |e| events.push(e));
    assert!(matches!(
        events.as_slice(),
        [
            Event::Text('x'),
            Event::Text('y'),
            Event::Key(Key {
                code: KeyCode::Escape,
                ..
            })
        ]
    ));
}

#[test]
fn arbitrary_bytes_are_chunk_independent() {
    let mut seed = 0x12345678u32;
    let mut bytes = [0; 512];
    for _ in 0..128 {
        for byte in &mut bytes {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            *byte = seed as u8;
        }
        let mut whole = Parser::default();
        let mut chunked = Parser::default();
        let mut expected = Vec::new();
        let mut actual = Vec::new();
        whole.parse(&bytes, |e| expected.push(e));
        for chunk in bytes.chunks(7) {
            chunked.parse(chunk, |e| actual.push(e));
        }
        assert_eq!(actual, expected);
    }
}

fn key(code: KeyCode, modifiers: Modifiers, kind: KeyKind) -> Event {
    Event::Key(Key {
        code,
        shifted: None,
        modifiers,
        kind,
        text: false,
    })
}
