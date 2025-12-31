use clap::Parser;

use std::rc::Rc;

use parking_lot::Mutex;

/// Raw command line arguments.
#[derive(Parser)]
#[command(about)]
pub struct Args {
    /// Print the AST the console
    #[arg(long, default_value_t = false)]
    pub print_ast: bool,

    /// Print the bytecode to the console
    #[arg(long, default_value_t = false)]
    pub print_bytecode: bool,

    /// Print the bytecode for all RegExps to the console
    #[arg(long, default_value_t = false)]
    pub print_regexp_bytecode: bool,

    /// Parse as module instead of script
    #[arg(short, long, default_value_t = false)]
    pub module: bool,

    /// Whether to enable Annex B extensions
    #[arg(long, default_value_t = cfg!(feature = "annex_b"))]
    pub annex_b: bool,

    /// Expose global gc methods
    #[arg(long, default_value_t = false)]
    pub expose_gc: bool,

    /// Expose the test262 object
    #[arg(long, default_value_t = false)]
    pub expose_test_262: bool,

    /// The starting heap size, in bytes.
    #[arg(long)]
    pub heap_size: Option<usize>,

    /// Do not use colors when printing to terminal. Otherwise use colors if supported.
    #[arg(long, default_value_t = false)]
    pub no_color: bool,

    /// Print statistics about the parse phase
    #[arg(long, default_value_t = false)]
    pub parse_stats: bool,

    #[arg(required = true)]
    pub files: Vec<String>,
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

    /// Create new options from command line arguments.
    pub fn new_from_args(args: &Args) -> Self {
        OptionsBuilder::new()
            .annex_b(args.annex_b)
            .print_ast(args.print_ast)
            .print_bytecode(args.print_bytecode)
            .print_regexp_bytecode(args.print_regexp_bytecode)
            .heap_size(args.heap_size.unwrap_or(DEFAULT_HEAP_SIZE))
            .parse_stats(args.parse_stats)
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

use so2js::{
    common::{constants::DEFAULT_HEAP_SIZE, options::Options, wtf_8::Wtf8String},
    handle_scope, must_a,
    parser::source::Source,
    runtime::{
        abstract_operations::define_property_or_throw, alloc_error::AllocResult,
        gc_object::GcObject, intrinsics::intrinsics::Intrinsic, object_value::ObjectValue,
        test_262_object::Test262Object, BsResult, Context, ContextBuilder, EvalResult,
        PropertyDescriptor, PropertyKey, Realm, StackRoot, Value,
    },
};

pub fn print_error_message_and_exit(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1);
}

fn create_context(args: &Args) -> AllocResult<Context> {
    let cx = ContextBuilder::new()
        .set_options(Rc::new(Options::default()))
        .build()?;

    if args.expose_gc {
        GcObject::install(cx, cx.initial_realm())?;
    }

    if args.expose_test_262 {
        Test262Object::install(cx, cx.initial_realm())?;
    }

    #[cfg(feature = "gc_stress_test")]
    {
        let mut cx = cx;
        cx.enable_gc_stress_test();
    }

    Ok(cx)
}

// Wrapper to pretty print errors
// fn main() {
//     let args = Args::parse();
//     let cx = create_context(&args).expect("Failed to create initial Context");

//     cx.execute_then_drop(|cx| {
//         let result = evaluate(cx, &args);

//         #[cfg(feature = "handle_stats")]
//         println!("{:?}", cx.heap.info().handle_context().handle_stats());

//         unwrap_error_or_exit(cx, result);
//     })
// }

pub struct ConsoleObject;

impl ConsoleObject {
    fn new(mut cx: Context, realm: StackRoot<Realm>) -> AllocResult<StackRoot<ObjectValue>> {
        let mut object = ObjectValue::new(
            cx,
            Some(realm.get_intrinsic(Intrinsic::ObjectPrototype)),
            true,
        )?;
        let log = cx.alloc_string("log").unwrap().as_string();
        object.intrinsic_func(
            cx,
            PropertyKey::string(cx, log).unwrap().to_stack(),
            Self::log,
            1,
            realm,
        )?;

        Ok(object.to_stack())
    }

    /// Install the GC object on the realm's global object.
    pub fn install(cx: Context, realm: StackRoot<Realm>) -> AllocResult<()> {
        handle_scope!(cx, {
            let gc_object = GcObject::new(cx, realm)?;
            let desc = PropertyDescriptor::data(gc_object.as_value(), true, false, true);
            must_a!(define_property_or_throw(
                cx,
                realm.global_object(),
                cx.names.gc(),
                desc
            ));

            Ok(())
        })
    }

    pub fn log(
        cx: Context,
        _: StackRoot<Value>,
        arguments: &[StackRoot<Value>],
    ) -> EvalResult<StackRoot<Value>> {
        let mut formatted = vec![];
        use so2js::runtime::to_console_string;
        for argument in arguments.iter() {
            formatted.push(to_console_string(cx, *argument)?);
        }

        println!("{}", formatted.join(" "));

        Ok(cx.undefined())
    }
}

fn main() {
    let mut cx = ContextBuilder::new()
        .set_options(Rc::new(Options::default()))
        .build()
        .unwrap();
    GcObject::install(cx, cx.initial_realm()).unwrap();

    ConsoleObject::install(cx, cx.initial_realm()).unwrap();
    loop {
        handle_scope!(cx, {
            // take one line of input from stdin
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap();
            let handle = cx.evaluate_script(Rc::new(
                Source::new_for_string("<repl>", Wtf8String::from_string(input)).unwrap(),
            ));
            println!("{:?}", handle);
            if let Err(err) = handle {
                println!("Error: {}", err.format(cx));
            }
        })
    }
}
