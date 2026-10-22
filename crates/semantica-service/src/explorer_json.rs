//! Syntax diagnostics for the locked original HTTP JSON decoder. This scanner
//! does not construct values: conversion stays in the existing JSON owner.
use serde_json::{Value, json};
use std::{io, time::Instant};
enum Fault {
    Syntax(usize, &'static str),
    Budget,
}
struct Scanner<'a> {
    text: &'a str,
    at: usize,
    deadline: Instant,
}
impl Scanner<'_> {
    fn check(&self) -> Result<(), Fault> {
        if Instant::now() >= self.deadline {
            Err(Fault::Budget)
        } else {
            Ok(())
        }
    }
    fn byte(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }
    fn space(&mut self) {
        while self.byte().is_some_and(|b| b" \n\r\t".contains(&b)) {
            self.at += 1;
        }
    }
    fn fail<T>(&self, message: &'static str) -> Result<T, Fault> {
        Err(Fault::Syntax(self.at, message))
    }
    fn string(&mut self) -> Result<(), Fault> {
        let start = self.at;
        self.at += 1;
        while let Some(b) = self.byte() {
            self.check()?;
            match b {
                b'"' => {
                    self.at += 1;
                    return Ok(());
                }
                0..=31 => return self.fail("Invalid control character at"),
                b'\\' => {
                    let slash = self.at;
                    self.at += 1;
                    match self.byte() {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                            self.at += 1
                        }
                        Some(b'u') => {
                            let u = self.at;
                            self.at += 1;
                            for _ in 0..4 {
                                if !self.byte().is_some_and(|b| b.is_ascii_hexdigit()) {
                                    return Err(Fault::Syntax(u, "Invalid \\uXXXX escape"));
                                }
                                self.at += 1;
                            }
                        }
                        None => {
                            return Err(Fault::Syntax(start, "Unterminated string starting at"));
                        }
                        _ => return Err(Fault::Syntax(slash, "Invalid \\escape")),
                    }
                }
                _ => self.at += 1,
            }
        }
        Err(Fault::Syntax(start, "Unterminated string starting at"))
    }
    fn number(&mut self) -> Result<(), Fault> {
        let start = self.at;
        if self.byte() == Some(b'-') {
            self.at += 1;
        }
        match self.byte() {
            Some(b'0') => self.at += 1,
            Some(b'1'..=b'9') => {
                while self.byte().is_some_and(|b| b.is_ascii_digit()) {
                    self.at += 1;
                }
            }
            _ => return Err(Fault::Syntax(start, "Expecting value")),
        }
        if self.byte() == Some(b'.')
            && self
                .text
                .as_bytes()
                .get(self.at + 1)
                .is_some_and(u8::is_ascii_digit)
        {
            self.at += 1;
            while self.byte().is_some_and(|b| b.is_ascii_digit()) {
                self.at += 1;
            }
        }
        if matches!(self.byte(), Some(b'e' | b'E')) {
            let saved = self.at;
            self.at += 1;
            if matches!(self.byte(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            if !self.byte().is_some_and(|b| b.is_ascii_digit()) {
                self.at = saved;
            } else {
                while self.byte().is_some_and(|b| b.is_ascii_digit()) {
                    self.at += 1;
                }
            }
        }
        Ok(())
    }
    fn value(&mut self, depth: usize) -> Result<(), Fault> {
        self.check()?;
        if depth > 128 {
            return Err(Fault::Budget);
        }
        self.space();
        match self.byte() {
            Some(b'"') => self.string(),
            Some(b'{') => {
                self.at += 1;
                self.space();
                if self.byte() == Some(b'}') {
                    self.at += 1;
                    return Ok(());
                }
                loop {
                    self.check()?;
                    if self.byte() != Some(b'"') {
                        return self.fail("Expecting property name enclosed in double quotes");
                    }
                    self.string()?;
                    self.space();
                    if self.byte() != Some(b':') {
                        return self.fail("Expecting ':' delimiter");
                    }
                    self.at += 1;
                    self.value(depth + 1)?;
                    self.space();
                    match self.byte() {
                        Some(b'}') => {
                            self.at += 1;
                            return Ok(());
                        }
                        Some(b',') => {
                            let comma = self.at;
                            self.at += 1;
                            self.space();
                            if self.byte() == Some(b'}') {
                                return Err(Fault::Syntax(
                                    comma,
                                    "Illegal trailing comma before end of object",
                                ));
                            }
                        }
                        _ => return self.fail("Expecting ',' delimiter"),
                    }
                }
            }
            Some(b'[') => {
                self.at += 1;
                self.space();
                if self.byte() == Some(b']') {
                    self.at += 1;
                    return Ok(());
                }
                loop {
                    self.value(depth + 1)?;
                    self.space();
                    match self.byte() {
                        Some(b']') => {
                            self.at += 1;
                            return Ok(());
                        }
                        Some(b',') => {
                            let comma = self.at;
                            self.at += 1;
                            self.space();
                            if self.byte() == Some(b']') {
                                return Err(Fault::Syntax(
                                    comma,
                                    "Illegal trailing comma before end of array",
                                ));
                            }
                        }
                        _ => return self.fail("Expecting ',' delimiter"),
                    }
                }
            }
            _ => {
                for token in ["true", "false", "null", "NaN", "Infinity", "-Infinity"] {
                    if self.text[self.at..].starts_with(token) {
                        self.at += token.len();
                        return Ok(());
                    }
                }
                if matches!(self.byte(), Some(b'-' | b'0'..=b'9')) {
                    self.number()
                } else {
                    self.fail("Expecting value")
                }
            }
        }
    }
}
pub(super) fn syntax_error(text: &str, deadline: Instant) -> io::Result<Option<Value>> {
    let mut scanner = Scanner {
        text,
        at: 0,
        deadline,
    };
    let result = scanner.value(0).and_then(|_| {
        scanner.space();
        scanner.check()?;
        if scanner.at < text.len() {
            scanner.fail("Extra data")
        } else {
            Ok(())
        }
    });
    match result {
        Ok(()) => Ok(None),
        Err(Fault::Budget) => Err(io::Error::other("http_json_syntax_budget")),
        Err(Fault::Syntax(position, message)) => {
            let position = text[..position].chars().count();
            Ok(Some(
                json!({"detail":[{"type":"json_invalid","loc":["body",position],"msg":"JSON decode error","input":{},"ctx":{"error":message}}]}),
            ))
        }
    }
}
