use alloc::string::String;
use parking_lot::{Mutex, MutexGuard};

use super::constants::DEFAULT_HEAP_SIZE;

/// Options passed throughout the program.
pub struct Options {
    /// Whether Annex B extensions are enabled
    pub annex_b: bool,

    /// Print each AST to the console
    pub print_ast: bool,

    /// Print the bytecode to the console
    pub print_bytecode: bool,

    /// Print the bytecode for all RegExps to the console
    pub print_regexp_bytecode: bool,

    /// Buffer to write all dumped output into instead of stdout
    pub dump_buffer: Option<Mutex<String>>,

    /// The heap size to use in bytes.
    pub heap_size: usize,

    /// Whether to use colors when printing to the terminal
    pub parse_stats: bool,
}

impl Options {
    // /// Create a new options struct from command line arguments.
    // pub fn new_from_args(args: &Args) -> Self {
    //     OptionsBuilder::new_from_args(args).build()
    // }

    pub fn dump_buffer(&self) -> Option<MutexGuard<'_, String>> {
        self.dump_buffer.as_ref().map(|buffer| buffer.lock())
    }
}

impl Default for Options {
    /// Create a new options struct with default values.
    fn default() -> Self {
        OptionsBuilder::new().build()
    }
}

pub struct OptionsBuilder(Options);

impl OptionsBuilder {
    /// Create new options with default values.
    pub fn new() -> Self {
        Self(Options {
            annex_b: cfg!(feature = "annex_b"),
            print_ast: false,
            print_bytecode: false,
            print_regexp_bytecode: false,
            dump_buffer: None,
            heap_size: DEFAULT_HEAP_SIZE,
            parse_stats: false,
        })
    }

    /// Return the options that have been built, consuming the builder.
    pub fn build(self) -> Options {
        self.0
    }

    pub fn annex_b(mut self, annex_b: bool) -> Self {
        self.0.annex_b = annex_b;
        self
    }

    pub fn print_ast(mut self, print_ast: bool) -> Self {
        self.0.print_ast = print_ast;
        self
    }

    pub fn print_bytecode(mut self, print_bytecode: bool) -> Self {
        self.0.print_bytecode = print_bytecode;
        self
    }

    pub fn print_regexp_bytecode(mut self, print_regexp_bytecode: bool) -> Self {
        self.0.print_regexp_bytecode = print_regexp_bytecode;
        self
    }

    pub fn heap_size(mut self, heap_size: usize) -> Self {
        self.0.heap_size = heap_size;
        self
    }

    pub fn dump_buffer(mut self, dump_buffer: Option<Mutex<String>>) -> Self {
        self.0.dump_buffer = dump_buffer;
        self
    }

    pub fn parse_stats(mut self, parse_stats: bool) -> Self {
        self.0.parse_stats = parse_stats;
        self
    }
}
