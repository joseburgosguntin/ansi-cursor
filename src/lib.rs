use ansi_parser::{AnsiParseIterator, AnsiParser, Output};
use std::io::Read;

#[derive(Debug)]
pub struct Cursor {
    pub x: usize,
    pub y: usize,
}

/// # Example
/// ```rust
/// use std::{
///    io::BufReader,
///    process::{Command, Stdio},
///};
///
///use ansi_cursor::{AnsiParseReaderIterator, Cursor};
///use ansi_parser::{AnsiSequence::*, Output};
///
/// // your terminal width
///const WIDTH: usize = 80usize;
///
/// // your handler for outputing the current display_buf
///fn on_output(buf: &[u8]) {
///    let mut lines = Vec::with_capacity(25);
///    for line in buf.chunks(WIDTH) {
///        lines.push(unsafe { std::str::from_utf8_unchecked(line) })
///    }
///    for line in lines {
///        println!("{line}")
///    }
///}
///
/// // your handler for each ansi sequence or &str
///fn apply_output(
///    output: Output,
///    display_buf: &mut [u8; 2000],
///    Cursor { x, y }: &mut Cursor,
///    width: usize,
///    on_output: impl Fn(&[u8]),
///) {
///    match output {
///        Output::TextBlock(text) => {
///            let bytes = text.as_bytes();
///            let len = bytes.len();
///            display_buf[*x + *y * width..*x + *y * width + len].copy_from_slice(bytes);
///
///            *y += len / width;
///            *x += len % width;
///        }
///        Output::Escape(sequence) => match sequence {
///            CursorPos(y_2, x_2) => {
///                *x = x_2 as usize;
///                *y = y_2 as usize;
///            }
///            CursorUp(n) => *y += n as usize,
///            CursorDown(n) => *y -= n as usize,
///            CursorForward(n) => *x += n as usize,
///            CursorBackward(n) => *x -= n as usize,
///            EraseLine => {
///                let erase_start = *x + (*y * width);
///                let len = width - *x;
///                for byte in &mut display_buf[erase_start..erase_start + len] {
///                    *byte = b' '
///                }
///            }
///            EraseDisplayFromCursor => {
///                let erase_start = *x + (*y * width);
///                for byte in &mut display_buf[erase_start..] {
///                    *byte = b' '
///                }
///            }
///            other => eprintln!("{other:?} was not handled"),
///        },
///    }
///
///    on_output(display_buf);
///}
///
///fn main() {
///    let mut child = Command::new("sshpass")
///        .args([
///            "-p",
///            "", // no password, change it with env var instead
///            "ssh",
///            "-oHostKeyAlgorithms=+ssh-rsa",
///            "example@example.example.example",
///            "-tt",
///        ])
///        .stdin(Stdio::piped())
///        .stdout(Stdio::piped())
///        .spawn()
///        .unwrap();
///
///    let Some(stdout) = child.stdout.take() else {
///        eprintln!("failed to get stdout");
///        return;
///    };
///
///    let mut cursor = Cursor { x: 0, y: 0 };
///    let mut display_buf = [0u8; 25 * WIDTH];
///
///    let reader = BufReader::new(stdout);
///    for res in AnsiParseReaderIterator::from(reader) {
///        break;
///        match res {
///            Ok(output) => apply_output(output, &mut display_buf, &mut cursor, WIDTH, on_output),
///            Err(e) => {
///                eprintln!("{e}");
///                return;
///            }
///        }
///    }
///}
/// ```
pub struct AnsiParseReaderIterator<'a> {
    reader: std::io::BufReader<std::process::ChildStdout>, // for now
    mixed_buf: [u8; 2usize.pow(7)],
    text_buf: [u8; 2usize.pow(7)],
    parser: AnsiParseIterator<'a>,
    past: Option<Output<'a>>,
    start: usize,
}

impl<'a> Iterator for AnsiParseReaderIterator<'a> {
    type Item = std::io::Result<Output<'a>>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            // reader failed to produce first output in next buffer
            let Some(current) = self.past.clone() else {
                return None;
            };

            // in the middle of the buf
            if let Some(next) = self.parser.next() {
                self.past = Some(next);
                return Some(Ok(current));
            }

            match current {
                // valid last output from buf
                Output::Escape(_) => {
                    self.start = 0;
                    // return Some(Ok(Output::Escape(escape)));
                }
                // posibly invalid
                Output::TextBlock(text) => {
                    self.text_buf[..text.len()].copy_from_slice(text.as_bytes());
                    self.start = text.len();
                    self.mixed_buf[..self.start].copy_from_slice(&self.text_buf[..self.start]);
                }
            }

            // this section tries to make a next buffer
            let n = match self.reader.read(&mut self.mixed_buf[self.start..]) {
                // failed to make next buffer
                Ok(0) => return None,
                // ready to make next buffer
                Ok(n) => n,
                Err(e) => return Some(Err(e)),
            };
            let mixed_str = unsafe {
                std::mem::transmute::<&str, &'a str>(std::str::from_utf8_unchecked(
                    &self.mixed_buf[..n],
                ))
            };
            self.parser = dbg!(mixed_str.ansi_parse());
            self.past = dbg!(self.parser.next());

            if let Output::Escape(escape) = current {
                return Some(Ok(Output::Escape(escape)));
            }
        }
    }
}

impl<'a> From<std::io::BufReader<std::process::ChildStdout>> for AnsiParseReaderIterator<'a> {
    fn from(mut value: std::io::BufReader<std::process::ChildStdout>) -> Self {
        let mut mixed_buf: [u8; 2usize.pow(7)] = [0; 2usize.pow(7)];
        let text_buf: [u8; 2usize.pow(7)] = [0; 2usize.pow(7)];
        let start = 0;

        let (past, parser) = match value.read(&mut mixed_buf[start..]) {
            // since past is None here the broken ansi_parse woudn't be a problem
            Ok(0) => dbg!((None, "".ansi_parse())),
            Ok(n) => {
                let mixed_str = unsafe {
                    // BUG: way too unsafe self.past's value is fucked
                    // UB cuz when some dbg! are added it works
                    /* std::mem::transmute::<&str, &'a str>( */
                    std::str::from_utf8_unchecked(&mixed_buf[..n]) /* ) */
                    // let x = &mixed_buf[..n];
                };
                // let mixed_str = std::rc::Rc::<str>::from("hey");
                let mixed_str = std::rc::Rc::<str>::from(mixed_str);
                let mixed_str = unsafe { (&*mixed_str as *const str).as_ref().unwrap() };

                let mut parser = mixed_str.ansi_parse();
                (parser.next(), parser)
            }
            // io err
            Err(_) => todo!(),
        };

        Self {
            parser,
            start,
            text_buf,
            mixed_buf,
            reader: value,
            past,
        }
    }
}

// pub async fn ansi_cursor<F, R>(
//     reader: &mut BufReader<ChildStdout>,
//     display_buf: &mut [u8],
//     Cursor { x, y }: &mut Cursor,
//     width: usize,
//     _height: usize,
//     on_output: F,
// ) where
//     F: Fn(&str) -> R,
//     R: Future<Output = ()>,
// {
//     // only that last output of parse can be in middle of esc sequence
//     // [ x x x  | e y y y y | x x x x x x | e y y y y y | x x x x x | e y # y y ]
//     let mut ansi_buf = [0u8; 2usize.pow(7)];
//     // contans pieces before the # above
//     // e y # ...
//     let mut maybe_text_buf = [0u8; 2usize.pow(7)];
//     // % tells where to start if there was maybe_text last iteration
//     // [ e y % y y x  | e y y y y | x x x x x x | e y y y y y | x x x x x | e y # y y ]
//     let mut start = 0;
//
//     loop {
//         let n = match reader.read(&mut ansi_buf[start..]).await {
//             Ok(0) => {
//                 // todo!("cond var stuff")
//                 continue;
//             }
//             Ok(n) => n,
//             Err(_) => {
//                 todo!("io err i guess")
//             }
//         };
//         let str = unsafe { std::str::from_utf8_unchecked(&ansi_buf[..n]) };
//
//         let mut parser = str.ansi_parse();
//         let mut last = parser
//             .next()
//             .expect("should always return a str that may contain esc");
//
//         // TODO: use ansi-cursor crate (mine)
//         let mut apply_output = |output: Output| {
//             match output {
//                 Output::TextBlock(text) => {
//                     let bytes = text.as_bytes();
//                     let len = bytes.len();
//                     display_buf[*x + *y * width..*x + *y * width + len].copy_from_slice(bytes);
//
//                     *y += len / width;
//                     *x += len % width;
//                 }
//                 Output::Escape(sequence) => match sequence {
//                     CursorPos(y_2, x_2) => {
//                         *x = x_2 as usize;
//                         *y = y_2 as usize;
//                     }
//                     CursorUp(n) => *y += n as usize,
//                     CursorDown(n) => *y -= n as usize,
//                     CursorForward(n) => *x += n as usize,
//                     CursorBackward(n) => *x -= n as usize,
//                     EraseLine => {
//                         let erase_start = *x + (*y * width);
//                         let len = width - *x;
//                         for byte in &mut display_buf[erase_start..erase_start + len] {
//                             *byte = b' '
//                         }
//                     }
//                     EraseDisplayFromCursor => {
//                         let erase_start = *x + (*y * width);
//                         for byte in &mut display_buf[erase_start..] {
//                             *byte = b' '
//                         }
//                     }
//                     other => eprintln!("{other:?} was not handled"),
//                 },
//             }
//
//             let display_str = unsafe { std::str::from_utf8_unchecked(display_buf) };
//             on_output(display_str);
//         };
//         for output in parser {
//             // at this point last shouldn't contain any esc cuz it already
//             // created a next output that may have it
//             let current = last;
//             last = output;
//             apply_output(current);
//         }
//         match last {
//             Output::TextBlock(text) => {
//                 // put in seperate buf to avoid reading from where we are writting
//                 maybe_text_buf[..text.len()].copy_from_slice(text.as_bytes());
//                 start = text.len();
//                 ansi_buf[..start].copy_from_slice(&maybe_text_buf[..start]);
//             }
//             Output::Escape(sequence) => {
//                 start = 0;
//                 apply_output(Output::Escape(sequence));
//             }
//         }
//     }
// }
//
// #[cfg(test)]
// mod tests {}
