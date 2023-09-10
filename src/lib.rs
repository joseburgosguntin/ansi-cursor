use ansi_parser::{AnsiParser, AnsiSequence::*, Output};
use tokio::{
    io::{AsyncReadExt, BufReader},
    process::ChildStdout,
};

#[derive(Debug)]
pub struct Cursor {
    pub x: usize,
    pub y: usize,
}

pub async fn ansi_cursor<F>(
    reader: &mut BufReader<ChildStdout>,
    display_buf: &mut [u8],
    Cursor { x, y }: &mut Cursor,
    width: usize,
    _height: usize,
    on_output: F,
) where
    F: Fn(&str),
{
    // only that last output of parse can be in middle of esc sequence
    // [ x x x  | e y y y y | x x x x x x | e y y y y y | x x x x x | e y # y y ]
    let mut ansi_buf = [0u8; 2usize.pow(7)];
    // contans pieces before the # above
    // e y # ...
    let mut maybe_text_buf = [0u8; 2usize.pow(7)];
    // % tells where to start if there was maybe_text last iteration
    // [ e y % y y x  | e y y y y | x x x x x x | e y y y y y | x x x x x | e y # y y ]
    let mut start = 0;

    loop {
        unsafe { std::str::from_utf8_unchecked(reader.buffer()) };

        let n = match reader.read(&mut ansi_buf[start..]).await {
            Ok(0) => {
                // todo!("cond var stuff")
                continue;
            }
            Ok(n) => n,
            Err(_) => {
                todo!("io err i guess")
            }
        };
        let str = unsafe { std::str::from_utf8_unchecked(&ansi_buf[..n]) };

        let mut parser = str.ansi_parse();
        let mut last = parser
            .next()
            .expect("should always return a str that may contain esc");

        // TODO: use ansi-cursor crate (mine)
        let mut apply_output = |output: Output| {
            match output {
                Output::TextBlock(text) => {
                    let bytes = text.as_bytes();
                    let len = bytes.len();
                    display_buf[*x + *y * width..*x + *y * width + len].copy_from_slice(bytes);

                    *y += len / width;
                    *x += len % width;
                }
                Output::Escape(sequence) => match sequence {
                    CursorPos(y_2, x_2) => {
                        *x = x_2 as usize;
                        *y = y_2 as usize;
                    }
                    CursorUp(n) => *y += n as usize,
                    CursorDown(n) => *y -= n as usize,
                    CursorForward(n) => *x += n as usize,
                    CursorBackward(n) => *x -= n as usize,
                    EraseLine => {
                        let erase_start = *x + (*y * width);
                        let len = width - *x;
                        for byte in &mut display_buf[erase_start..erase_start + len] {
                            *byte = b' '
                        }
                    }
                    EraseDisplayFromCursor => {
                        let erase_start = *x + (*y * width);
                        for byte in &mut display_buf[erase_start..] {
                            *byte = b' '
                        }
                    }
                    other => eprintln!("{other:?} was not handled"),
                },
            }

            let display_str = unsafe { std::str::from_utf8_unchecked(display_buf) };

            on_output(display_str);
        };
        for output in parser {
            // at this point last shouldn't contain any esc cuz it already
            // created a next output that may have it
            let current = last;
            last = output;
            apply_output(current);
        }
        match last {
            Output::TextBlock(text) => {
                // put in seperate buf to avoid reading from where we are writting
                maybe_text_buf[..text.len()].copy_from_slice(text.as_bytes());
                start = text.len();
                ansi_buf[..start].copy_from_slice(&maybe_text_buf[..start]);
            }
            Output::Escape(sequence) => {
                start = 0;
                apply_output(Output::Escape(sequence));
            }
        }
    }
}
