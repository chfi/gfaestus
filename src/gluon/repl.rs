use std::{path::Path, sync::Arc};

use bstr::ByteVec;
use crossbeam::channel::Sender;
use futures::Future;
use gluon_codegen::*;

use gluon::{
    base::{
        ast::{
            self, AstClone, Expr, Pattern, RootExpr, SpannedPattern, Typed,
            TypedIdent,
        },
        error::InFile,
        kind::Kind,
        mk_ast_arena, pos, resolve,
        symbol::{Symbol, SymbolModule},
        types::{ArcType, TypeExt},
        DebugLevel,
    },
    compiler_pipeline::{Executable, ExecuteValue},
    import::add_extern_module,
    query::CompilerDatabase,
    vm::{
        api::{FunctionRef, Hole, OpaqueValue, WithVM, IO},
        ExternModule, {self, Error as VMError, Result as VMResult},
    },
    Error as GluonError, Result as GluonResult, RootedThread, ThreadExt,
};

use gluon_completion as completion;

use vm::api::Function;

use anyhow::Result;

use crate::{
    app::{mainview::MainViewMsg, AppMsg},
    geometry::Point,
    view::View,
};

fn type_of_expr(
    args: WithVM<&str>,
) -> impl Future<Output = IO<std::result::Result<String, String>>> {
    let WithVM { vm, value: args } = args;
    let args = args.to_string();
    let vm = vm.new_thread().unwrap(); // TODO Run on the same thread once that works

    async move {
        IO::Value(match vm.typecheck_str_async("<repl>", &args, None).await {
            Ok((expr, _)) => {
                let env = vm.get_env();
                Ok(format!("{}", expr.env_type_of(&env)))
            }
            Err(msg) => Err(format!("{}", msg)),
        })
    }
}

fn find_kind(args: WithVM<&str>) -> IO<Result<String>> {
    let vm = args.vm;
    let args = args.value.trim();
    IO::Value(match vm.find_type_info(args) {
        Ok(ref alias) => {
            let kind =
                alias.params().iter().rev().fold(Kind::typ(), |acc, arg| {
                    Kind::function(arg.kind.clone(), acc)
                });
            Ok(format!("{}", kind))
        }
        Err(err) => Err(format!("{}", err)),
    })
}

// fn find_info(args: WithVM<&str>) -> IO<Result<String, String>> {
fn find_info(args: WithVM<&str>) -> IO<Result<String>> {
    use std::fmt::Write;
    let vm = args.vm;
    let args = args.value.trim();
    let env = vm.get_env();
    let mut buffer = String::new();
    match env.find_type_info(args) {
        Ok(alias) => {
            // Found a type alias
            let mut fmt = || -> Result<(), std::fmt::Error> {
                write!(&mut buffer, "type {}", args)?;
                for g in alias.params() {
                    write!(&mut buffer, " {}", g.id)?;
                }
                write!(&mut buffer, " = {}", alias.unresolved_type())
            };
            fmt().unwrap();
        }
        Err(err) => {
            // Try to find a value at `args` to print its type and documentation comment (if any)
            match env.get_binding(args) {
                Ok((_, typ)) => {
                    write!(&mut buffer, "{}: {}", args, typ).unwrap();
                }
                Err(_) => return IO::Value(Err(format!("{}", err))),
            }
        }
    }
    let maybe_metadata = env.get_metadata(args).ok();
    if let Some(comment) = maybe_metadata
        .as_ref()
        .and_then(|metadata| metadata.comment.as_ref())
    {
        for line in comment.content.lines() {
            write!(&mut buffer, "\n/// {}", line).unwrap();
        }
    }
    IO::Value(Ok(buffer))
}

fn switch_debug_level(args: WithVM<&str>) -> IO<Result<String, String>> {
    let vm = args.vm;
    let args = args.value.trim();
    if args != "" {
        let debug_level = match DebugLevel::from_str(args) {
            Ok(debug_level) => debug_level,
            Err(e) => return IO::Value(Err(e.to_string())),
        };
        vm.global_env().set_debug_level(debug_level);
    }
    IO::Value(Ok(vm.global_env().get_debug_level().to_string()))
}

fn complete(
    thread: &Thread,
    name: &str,
    fileinput: &str,
    pos: usize,
) -> GluonResult<Vec<String>> {
    use gluon::compiler_pipeline::*;

    let mut db = thread.get_database();
    let mut module_compiler = thread.module_compiler(&mut db);

    // The parser may find parse errors but still produce an expression
    // For that case still typecheck the expression but return the parse error afterwards
    let mut expr = match parse_expr(
        &mut module_compiler,
        thread.global_env().type_cache(),
        &name,
        fileinput,
    ) {
        Ok(expr) => expr,
        Err(err) => err.get_value()?,
    };

    // Only need the typechecker to fill infer the types as best it can regardless of errors
    let _ =
        (&mut expr).typecheck(&mut module_compiler, thread, &name, fileinput);
    let file_map = module_compiler.get_filemap(&name).ok_or_else(|| {
        VMError::from("FileMap is missing for completion".to_string())
    })?;
    let suggestions = completion::suggest(
        &thread.get_env(),
        file_map.span(),
        &expr.expr(),
        file_map.span().start() + pos::ByteOffset::from(pos as i64),
    );
    Ok(suggestions
        .into_iter()
        .map(|ident| {
            let s: &str = ident.name.as_ref();
            s.to_string()
        })
        .collect())
}
