use std::{path::Path, str::FromStr, sync::Arc};

use bstr::ByteVec;
use crossbeam::channel::Sender;
use futures::{Future, FutureExt};
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
        vm::RootedValue,
        ExternModule, {self, Error as VMError, Result as VMResult},
    },
    Error as GluonError, Result as GluonResult, RootedThread, Thread,
    ThreadExt,
};

use gluon::parser::{parse_partial_repl_line, ReplLine};

use gluon_completion as completion;

use vm::api::Function;

use anyhow::Result;

use crate::{
    app::{mainview::MainViewMsg, AppMsg},
    geometry::Point,
    view::View,
};

// taken and modified from gluon_repl

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

fn find_kind(args: WithVM<&str>) -> IO<std::result::Result<String, String>> {
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

fn find_info(args: WithVM<&str>) -> IO<std::result::Result<String, String>> {
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

fn eval_line(
    WithVM { vm, value: line }: WithVM<&str>,
) -> impl Future<Output = IO<()>> {
    let vm = vm.new_thread().unwrap(); // TODO Reuse the current thread
    let line = line.to_string();
    async move {
        eval_line_(vm.root_thread(), &line)
            .map(move |result| match result {
                Ok(x) => IO::Value(x),
                Err(err) => {
                    // if let Err(err) = err.emit(&mut stderr) {
                    eprintln!("{}", err);
                    // }
                    IO::Value(())
                }
            })
            .await
    }
}

async fn eval_line_(vm: RootedThread, line: &str) -> gluon::Result<()> {
    let mut is_let_binding = false;
    let mut eval_expr;
    let value = {
        let mut db = vm.get_database();
        let mut module_compiler = vm.module_compiler(&mut db);
        eval_expr = {
            let eval_expr = {
                mk_ast_arena!(arena);
                let repl_line = {
                    let result = {
                        let filemap =
                            vm.get_database().add_filemap("line", line);
                        let mut module = SymbolModule::new(
                            "line".into(),
                            module_compiler.mut_symbols(),
                        );
                        parse_partial_repl_line(
                            (*arena).borrow(),
                            &mut module,
                            &*filemap,
                        )
                    };
                    match result {
                        Ok(x) => x,
                        Err((_, err)) => {
                            let code_map = db.code_map();
                            return Err(InFile::new(code_map, err).into());
                        }
                    }
                };
                match repl_line {
                    None => return Ok(()),
                    Some(ReplLine::Expr(expr)) => {
                        RootExpr::new(arena.clone(), arena.alloc(expr))
                    }
                    Some(ReplLine::Let(let_binding)) => {
                        is_let_binding = true;
                        // We can't compile function bindings by only looking at `let_binding.expr`
                        // so rewrite `let f x y = <expr>` into `let f x y = <expr> in f`
                        // and `let { x } = <expr>` into `let repl_temp @ { x } = <expr> in repl_temp`
                        let id = match let_binding.name.value {
                            Pattern::Ident(ref id)
                                if !let_binding.args.is_empty() =>
                            {
                                id.clone()
                            }
                            _ => {
                                let id = Symbol::from("repl_temp");
                                let_binding.name = pos::spanned(
                                    let_binding.name.span,
                                    Pattern::As(
                                        pos::spanned(
                                            let_binding.name.span,
                                            id.clone(),
                                        ),
                                        arena.alloc(
                                            let_binding
                                                .name
                                                .ast_clone(arena.borrow()),
                                        ),
                                    ),
                                );
                                TypedIdent {
                                    name: id,
                                    typ: let_binding.resolved_type.clone(),
                                }
                            }
                        };
                        let id = pos::spanned2(
                            0.into(),
                            0.into(),
                            Expr::Ident(id.clone()),
                        );
                        let expr = Expr::LetBindings(
                            ast::ValueBindings::Plain(let_binding),
                            arena.alloc(id),
                        );
                        let eval_expr = RootExpr::new(
                            arena.clone(),
                            arena.alloc(pos::spanned2(
                                0.into(),
                                0.into(),
                                expr,
                            )),
                        );
                        eval_expr
                    }
                }
            };
            eval_expr.try_into_send().unwrap()
        };

        (&mut eval_expr)
            .run_expr(&mut module_compiler, vm.clone(), "line", line, None)
            .await?
    };
    let ExecuteValue { value, typ, .. } = value;

    if is_let_binding {
        let mut expr = eval_expr.expr();
        let mut last_bind = None;
        loop {
            match &expr.value {
                Expr::LetBindings(binds, body) => {
                    last_bind = Some(&binds[0]);
                    expr = body;
                }
                _ => break,
            }
        }
        set_globals(
            &vm,
            &mut vm.get_database_mut(),
            &last_bind.unwrap().name,
            &typ,
            &value.as_ref(),
        )?;
    }
    let vm = value.vm();
    let env = vm.get_env();
    let debug_level = vm.global_env().get_debug_level();
    // println!(
    //     "{}",
    //     ValuePrinter::new(&env, &typ, value.get_variant(), &debug_level)
    //         .width(80)
    //         .max_level(5)
    // );
    Ok(())
}

fn set_globals(
    vm: &Thread,
    db: &mut CompilerDatabase,
    pattern: &SpannedPattern<Symbol>,
    typ: &ArcType,
    value: &RootedValue<&Thread>,
) -> GluonResult<()> {
    match pattern.value {
        Pattern::Ident(ref id) => {
            db.set_global(
                id.name.declared_name(),
                typ.clone(),
                Default::default(),
                value.get_value(),
            );
            Ok(())
        }
        Pattern::Tuple { ref elems, .. } => {
            let iter = elems
                .iter()
                .zip(gluon::vm::dynamic::field_iter(&value, typ, vm));
            for (elem_pattern, (elem_value, elem_type)) in iter {
                set_globals(vm, db, elem_pattern, &elem_type, &elem_value)?;
            }
            Ok(())
        }
        Pattern::Record { ref fields, .. } => {
            let resolved_type = {
                let mut type_cache = vm.global_env().type_cache();
                let env = db.as_env();
                resolve::remove_aliases_cow(&env, &mut type_cache, typ)
            };

            for (name, pattern_value) in ast::pattern_values(fields) {
                let field_name: &Symbol = &name.value;
                // if the record didn't have a field with this name,
                // there should have already been a type error. So we can just panic here
                let field_value: RootedValue<&Thread> = value
                    .get_field(field_name.declared_name())
                    .unwrap_or_else(|| {
                        panic!(
                            "record doesn't have field `{}`",
                            field_name.declared_name()
                        )
                    });
                let field_type = resolved_type
                    .row_iter()
                    .find(|f| f.name.name_eq(field_name))
                    .unwrap_or_else(|| {
                        panic!(
                            "record type `{}` doesn't have field `{}`",
                            resolved_type,
                            field_name.declared_name()
                        )
                    })
                    .typ
                    .clone();
                match pattern_value {
                    Some(ref sub_pattern) => set_globals(
                        vm,
                        db,
                        sub_pattern,
                        &field_type,
                        &field_value,
                    )?,
                    None => db.set_global(
                        name.value.declared_name(),
                        field_type.to_owned(),
                        Default::default(),
                        field_value.get_value(),
                    ),
                }
            }
            Ok(())
        }
        Pattern::As(ref id, ref pattern) => {
            db.set_global(
                id.value.declared_name(),
                typ.clone(),
                Default::default(),
                value.get_value(),
            );
            set_globals(vm, db, pattern, typ, value)
        }
        Pattern::Constructor(..) | Pattern::Literal(_) | Pattern::Error => {
            Err(VMError::Message(
                "The repl cannot bind variables from this pattern".into(),
            )
            .into())
        }
    }
}
