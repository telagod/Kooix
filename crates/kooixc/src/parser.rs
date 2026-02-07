use crate::ast::{
    AgentDecl, AgentPolicy, CapabilityDecl, EffectSpec, EnsureClause, EvidenceSpec, FailureAction,
    FailureActionArg, FailurePolicy, FailureRule, FailureValue, FunctionDecl, Item, LoopSpec,
    OutputField, Param, PredicateOp, PredicateValue, Program, RecordDecl, RecordField, StateRule,
    TypeArg, TypeRef, WorkflowCall, WorkflowCallArg, WorkflowDecl, WorkflowStep,
};
use crate::error::{Diagnostic, Span};
use crate::token::{Token, TokenKind};

pub fn parse(tokens: &[Token]) -> Result<Program, Diagnostic> {
    Parser::new(tokens).parse_program()
}

struct Parser<'a> {
    tokens: &'a [Token],
    index: usize,
}

impl<'a> Parser<'a> {
    fn new(tokens: &'a [Token]) -> Self {
        Self { tokens, index: 0 }
    }

    fn parse_program(mut self) -> Result<Program, Diagnostic> {
        let mut items = Vec::new();
        while !self.at_eof() {
            let item = if self.at_kw_cap() {
                Item::Capability(self.parse_capability_decl()?)
            } else if self.at_kw_fn() {
                Item::Function(self.parse_function_decl()?)
            } else if self.at_kw_workflow() {
                Item::Workflow(self.parse_workflow_decl()?)
            } else if self.at_kw_agent() {
                Item::Agent(self.parse_agent_decl()?)
            } else if self.at_kw_record() {
                Item::Record(self.parse_record_decl()?)
            } else {
                return Err(Diagnostic::error(
                    format!(
                        "expected top-level declaration, found {}",
                        self.current_kind_name()
                    ),
                    self.current().span,
                ));
            };
            items.push(item);
        }

        Ok(Program { items })
    }

    fn parse_capability_decl(&mut self) -> Result<CapabilityDecl, Diagnostic> {
        let start = self.expect_kw_cap()?.start;
        let capability = self.parse_type_ref()?;
        let end = self.expect_semicolon()?.end;
        Ok(CapabilityDecl {
            capability,
            span: Span::new(start, end),
        })
    }

    fn parse_record_decl(&mut self) -> Result<RecordDecl, Diagnostic> {
        let start = self.expect_kw_record()?.start;
        let (name, _) = self.expect_ident()?;

        let mut generics = Vec::new();
        if self.at_langle() {
            self.expect_langle()?;
            if !self.at_rangle() {
                loop {
                    let (generic_name, _) = self.expect_ident()?;
                    generics.push(generic_name);
                    if self.at_comma() {
                        self.advance();
                        continue;
                    }
                    break;
                }
            }
            self.expect_rangle()?;
        }

        self.expect_lbrace()?;

        let mut fields = Vec::new();
        while !self.at_rbrace() {
            let (field_name, _) = self.expect_ident()?;
            self.expect_colon()?;
            let field_type = self.parse_type_ref()?;
            self.expect_semicolon()?;
            fields.push(RecordField {
                name: field_name,
                ty: field_type,
            });
        }

        self.expect_rbrace()?;
        let end = self.expect_semicolon()?.end;

        Ok(RecordDecl {
            name,
            generics,
            fields,
            span: Span::new(start, end),
        })
    }

    fn parse_function_decl(&mut self) -> Result<FunctionDecl, Diagnostic> {
        let start = self.expect_kw_fn()?.start;
        let (name, _) = self.expect_ident()?;

        self.expect_lparen()?;
        let mut params = Vec::new();
        if !self.at_rparen() {
            loop {
                params.push(self.parse_param()?);
                if self.at_comma() {
                    self.advance();
                    continue;
                }
                break;
            }
        }
        self.expect_rparen()?;

        self.expect_arrow()?;
        let return_type = self.parse_type_ref()?;

        let intent = if self.at_kw_intent() {
            self.parse_intent()?
        } else {
            None
        };

        let effects = if self.at_bang() {
            self.parse_effects()?
        } else {
            Vec::new()
        };

        let requires = if self.at_kw_requires() {
            self.parse_requires()?
        } else {
            Vec::new()
        };

        let ensures = if self.at_kw_ensures() {
            self.parse_ensures()?
        } else {
            Vec::new()
        };

        let failure = if self.at_kw_failure() {
            Some(self.parse_failure()?)
        } else {
            None
        };

        let evidence = if self.at_kw_evidence() {
            Some(self.parse_evidence()?)
        } else {
            None
        };

        let end = self.expect_semicolon()?.end;

        Ok(FunctionDecl {
            name,
            params,
            return_type,
            intent,
            effects,
            requires,
            ensures,
            failure,
            evidence,
            span: Span::new(start, end),
        })
    }

    fn parse_workflow_decl(&mut self) -> Result<WorkflowDecl, Diagnostic> {
        let start = self.expect_kw_workflow()?.start;
        let (name, _) = self.expect_ident()?;

        self.expect_lparen()?;
        let mut params = Vec::new();
        if !self.at_rparen() {
            loop {
                params.push(self.parse_param()?);
                if self.at_comma() {
                    self.advance();
                    continue;
                }
                break;
            }
        }
        self.expect_rparen()?;

        self.expect_arrow()?;
        let return_type = self.parse_type_ref()?;

        let intent = if self.at_kw_intent() {
            self.parse_intent()?
        } else {
            None
        };

        let requires = if self.at_kw_requires() {
            self.parse_requires()?
        } else {
            Vec::new()
        };

        let steps = self.parse_steps_block()?;

        let output = if self.at_kw_output() {
            self.parse_output_block()?
        } else {
            Vec::new()
        };

        let evidence = if self.at_kw_evidence() {
            Some(self.parse_evidence()?)
        } else {
            None
        };

        let end = self.expect_semicolon()?.end;

        Ok(WorkflowDecl {
            name,
            params,
            return_type,
            intent,
            requires,
            steps,
            output,
            evidence,
            span: Span::new(start, end),
        })
    }

    fn parse_agent_decl(&mut self) -> Result<AgentDecl, Diagnostic> {
        let start = self.expect_kw_agent()?.start;
        let (name, _) = self.expect_ident()?;

        self.expect_lparen()?;
        let mut params = Vec::new();
        if !self.at_rparen() {
            loop {
                params.push(self.parse_param()?);
                if self.at_comma() {
                    self.advance();
                    continue;
                }
                break;
            }
        }
        self.expect_rparen()?;

        self.expect_arrow()?;
        let return_type = self.parse_type_ref()?;

        let intent = if self.at_kw_intent() {
            self.parse_intent()?
        } else {
            None
        };

        let state_rules = self.parse_state_block()?;
        let policy = self.parse_policy_block()?;

        let requires = if self.at_kw_requires() {
            self.parse_requires()?
        } else {
            Vec::new()
        };

        let loop_spec = self.parse_loop_block()?;

        let ensures = if self.at_kw_ensures() {
            self.parse_ensures()?
        } else {
            Vec::new()
        };

        let evidence = if self.at_kw_evidence() {
            Some(self.parse_evidence()?)
        } else {
            None
        };

        let end = self.expect_semicolon()?.end;

        Ok(AgentDecl {
            name,
            params,
            return_type,
            intent,
            state_rules,
            policy,
            requires,
            loop_spec,
            ensures,
            evidence,
            span: Span::new(start, end),
        })
    }

    fn parse_intent(&mut self) -> Result<Option<String>, Diagnostic> {
        self.expect_kw_intent()?;
        let Some(intent) = self.take_string() else {
            return Err(Diagnostic::error(
                "expected string literal after 'intent'",
                self.current().span,
            ));
        };
        Ok(Some(intent))
    }

    fn parse_param(&mut self) -> Result<Param, Diagnostic> {
        let (name, _) = self.expect_ident()?;
        self.expect_colon()?;
        let ty = self.parse_type_ref()?;
        Ok(Param { name, ty })
    }

    fn parse_effects(&mut self) -> Result<Vec<EffectSpec>, Diagnostic> {
        self.expect_bang()?;
        self.expect_lbrace()?;
        let mut effects = Vec::new();

        if !self.at_rbrace() {
            loop {
                let (name, _) = self.expect_ident()?;
                let argument = if self.at_lparen() {
                    self.expect_lparen()?;
                    let value = if let Some(value) = self.take_ident() {
                        value
                    } else if let Some(value) = self.take_string() {
                        value
                    } else if let Some(value) = self.take_number() {
                        value
                    } else {
                        return Err(Diagnostic::error(
                            "expected effect argument",
                            self.current().span,
                        ));
                    };
                    self.expect_rparen()?;
                    Some(value)
                } else {
                    None
                };
                effects.push(EffectSpec { name, argument });

                if self.at_comma() {
                    self.advance();
                    continue;
                }
                break;
            }
        }

        self.expect_rbrace()?;
        Ok(effects)
    }

    fn parse_requires(&mut self) -> Result<Vec<TypeRef>, Diagnostic> {
        self.expect_kw_requires()?;
        self.expect_lbracket()?;
        let mut required = Vec::new();

        if !self.at_rbracket() {
            loop {
                required.push(self.parse_type_ref()?);
                if self.at_comma() {
                    self.advance();
                    continue;
                }
                break;
            }
        }

        self.expect_rbracket()?;
        Ok(required)
    }

    fn parse_ensures(&mut self) -> Result<Vec<EnsureClause>, Diagnostic> {
        self.expect_kw_ensures()?;
        self.expect_lbracket()?;
        let mut ensures = Vec::new();

        if !self.at_rbracket() {
            loop {
                ensures.push(self.parse_ensure_clause()?);
                if self.at_comma() {
                    self.advance();
                    continue;
                }
                break;
            }
        }

        self.expect_rbracket()?;
        Ok(ensures)
    }

    fn parse_failure(&mut self) -> Result<FailurePolicy, Diagnostic> {
        self.expect_kw_failure()?;
        self.expect_lbrace()?;
        let mut rules = Vec::new();

        while !self.at_rbrace() {
            rules.push(self.parse_failure_rule()?);
        }

        self.expect_rbrace()?;
        Ok(FailurePolicy { rules })
    }

    fn parse_failure_rule(&mut self) -> Result<FailureRule, Diagnostic> {
        let (condition, _) = self.expect_ident()?;
        self.expect_arrow()?;
        let action = self.parse_failure_action()?;
        self.expect_semicolon()?;
        Ok(FailureRule { condition, action })
    }

    fn parse_failure_action(&mut self) -> Result<FailureAction, Diagnostic> {
        let (name, _) = self.expect_ident()?;
        self.expect_lparen()?;
        let mut args = Vec::new();

        if !self.at_rparen() {
            loop {
                args.push(self.parse_failure_action_arg()?);
                if self.at_comma() {
                    self.advance();
                    continue;
                }
                break;
            }
        }

        self.expect_rparen()?;
        Ok(FailureAction { name, args })
    }

    fn parse_failure_action_arg(&mut self) -> Result<FailureActionArg, Diagnostic> {
        if self.at_ident() && self.peek_kind_is_eq() {
            let (key, _) = self.expect_ident()?;
            self.expect_eq()?;
            let value = self.parse_failure_value()?;
            return Ok(FailureActionArg {
                key: Some(key),
                value,
            });
        }

        let value = self.parse_failure_value()?;
        Ok(FailureActionArg { key: None, value })
    }

    fn parse_failure_value(&mut self) -> Result<FailureValue, Diagnostic> {
        if let Some(value) = self.take_ident() {
            return Ok(FailureValue::Ident(value));
        }
        if let Some(value) = self.take_string() {
            return Ok(FailureValue::String(value));
        }
        if let Some(value) = self.take_number() {
            return Ok(FailureValue::Number(value));
        }

        Err(Diagnostic::error(
            "expected failure action argument",
            self.current().span,
        ))
    }

    fn parse_evidence(&mut self) -> Result<EvidenceSpec, Diagnostic> {
        self.expect_kw_evidence()?;
        self.expect_lbrace()?;

        let mut trace = None;
        let mut metrics = Vec::new();

        while !self.at_rbrace() {
            if self.at_kw_trace() {
                self.expect_kw_trace()?;
                let Some(value) = self.take_string() else {
                    return Err(Diagnostic::error(
                        "expected string literal after 'trace'",
                        self.current().span,
                    ));
                };
                trace = Some(value);
                self.expect_semicolon()?;
                continue;
            }

            if self.at_kw_metrics() {
                self.expect_kw_metrics()?;
                self.expect_lbracket()?;
                metrics = self.parse_identifier_list()?;
                self.expect_rbracket()?;
                self.expect_semicolon()?;
                continue;
            }

            return Err(Diagnostic::error(
                format!(
                    "expected 'trace' or 'metrics' in evidence block, found {}",
                    self.current_kind_name()
                ),
                self.current().span,
            ));
        }

        self.expect_rbrace()?;
        Ok(EvidenceSpec { trace, metrics })
    }

    fn parse_steps_block(&mut self) -> Result<Vec<WorkflowStep>, Diagnostic> {
        self.expect_kw_steps()?;
        self.expect_lbrace()?;

        let mut steps = Vec::new();
        while !self.at_rbrace() {
            steps.push(self.parse_workflow_step()?);
        }

        self.expect_rbrace()?;
        Ok(steps)
    }

    fn parse_workflow_step(&mut self) -> Result<WorkflowStep, Diagnostic> {
        let (id, _) = self.expect_ident()?;
        self.expect_colon()?;
        let call = self.parse_workflow_call()?;

        let ensures = if self.at_kw_ensures() {
            self.parse_ensures()?
        } else {
            Vec::new()
        };

        let on_fail = if self.at_kw_on_fail() {
            self.expect_kw_on_fail()?;
            self.expect_arrow()?;
            Some(self.parse_failure_action()?)
        } else {
            None
        };

        self.expect_semicolon()?;
        Ok(WorkflowStep {
            id,
            call,
            ensures,
            on_fail,
        })
    }

    fn parse_workflow_call(&mut self) -> Result<WorkflowCall, Diagnostic> {
        let (target, _) = self.expect_ident()?;
        self.expect_lparen()?;

        let mut args = Vec::new();
        if !self.at_rparen() {
            loop {
                args.push(self.parse_workflow_call_arg()?);
                if self.at_comma() {
                    self.advance();
                    continue;
                }
                break;
            }
        }

        self.expect_rparen()?;
        Ok(WorkflowCall { target, args })
    }

    fn parse_workflow_call_arg(&mut self) -> Result<WorkflowCallArg, Diagnostic> {
        if let Some(value) = self.take_string() {
            return Ok(WorkflowCallArg::String(value));
        }
        if let Some(value) = self.take_number() {
            return Ok(WorkflowCallArg::Number(value));
        }
        if self.at_ident() {
            return Ok(WorkflowCallArg::Path(self.parse_symbol_path()?));
        }

        Err(Diagnostic::error(
            "expected workflow step argument",
            self.current().span,
        ))
    }

    fn parse_state_block(&mut self) -> Result<Vec<StateRule>, Diagnostic> {
        self.expect_kw_state()?;
        self.expect_lbrace()?;

        let mut rules = Vec::new();
        while !self.at_rbrace() {
            let from = if self.at_kw_any() {
                self.expect_kw_any()?;
                "any".to_string()
            } else {
                let (value, _) = self.expect_ident()?;
                value
            };

            self.expect_arrow()?;

            let mut to = Vec::new();
            loop {
                let (state, _) = self.expect_ident()?;
                to.push(state);
                if self.at_comma() {
                    self.advance();
                    continue;
                }
                break;
            }

            self.expect_semicolon()?;
            rules.push(StateRule { from, to });
        }

        self.expect_rbrace()?;
        Ok(rules)
    }

    fn parse_policy_block(&mut self) -> Result<AgentPolicy, Diagnostic> {
        self.expect_kw_policy()?;
        self.expect_lbrace()?;

        let mut allow_tools = Vec::new();
        let mut deny_tools = Vec::new();
        let mut max_iterations = None;
        let mut human_in_loop_when = None;

        while !self.at_rbrace() {
            if self.at_kw_allow_tools() {
                self.expect_kw_allow_tools()?;
                self.expect_lbracket()?;
                allow_tools = self.parse_string_list()?;
                self.expect_rbracket()?;
                self.expect_semicolon()?;
                continue;
            }

            if self.at_kw_deny_tools() {
                self.expect_kw_deny_tools()?;
                self.expect_lbracket()?;
                deny_tools = self.parse_string_list()?;
                self.expect_rbracket()?;
                self.expect_semicolon()?;
                continue;
            }

            if self.at_kw_max_iterations() {
                self.expect_kw_max_iterations()?;
                self.expect_eq()?;
                let Some(value) = self.take_number() else {
                    return Err(Diagnostic::error(
                        "expected number after 'max_iterations ='",
                        self.current().span,
                    ));
                };
                max_iterations = Some(value);
                self.expect_semicolon()?;
                continue;
            }

            if self.at_kw_human_in_loop() {
                self.expect_kw_human_in_loop()?;
                self.expect_kw_when()?;
                human_in_loop_when = Some(self.parse_ensure_clause()?);
                self.expect_semicolon()?;
                continue;
            }

            return Err(Diagnostic::error(
                format!(
                    "expected policy clause in agent policy block, found {}",
                    self.current_kind_name()
                ),
                self.current().span,
            ));
        }

        self.expect_rbrace()?;
        Ok(AgentPolicy {
            allow_tools,
            deny_tools,
            max_iterations,
            human_in_loop_when,
        })
    }

    fn parse_loop_block(&mut self) -> Result<LoopSpec, Diagnostic> {
        self.expect_kw_loop()?;
        self.expect_lbrace()?;

        let mut stages = Vec::new();
        let (first_stage, _) = self.expect_ident()?;
        stages.push(first_stage);
        while self.at_arrow() {
            self.expect_arrow()?;
            let (stage, _) = self.expect_ident()?;
            stages.push(stage);
        }
        self.expect_semicolon()?;

        self.expect_kw_stop()?;
        self.expect_kw_when()?;
        let stop_when = self.parse_ensure_clause()?;
        self.expect_semicolon()?;

        self.expect_rbrace()?;
        Ok(LoopSpec { stages, stop_when })
    }

    fn parse_output_block(&mut self) -> Result<Vec<OutputField>, Diagnostic> {
        self.expect_kw_output()?;
        self.expect_lbrace()?;

        let mut fields = Vec::new();
        while !self.at_rbrace() {
            let (name, _) = self.expect_ident()?;
            self.expect_colon()?;
            let ty = self.parse_type_ref()?;
            let source = if self.at_eq() {
                self.expect_eq()?;
                Some(self.parse_symbol_path()?)
            } else {
                None
            };
            self.expect_semicolon()?;
            fields.push(OutputField { name, ty, source });
        }

        self.expect_rbrace()?;
        Ok(fields)
    }

    fn parse_symbol_path(&mut self) -> Result<Vec<String>, Diagnostic> {
        let (head, _) = self.expect_ident()?;
        let mut segments = vec![head];
        while self.at_dot() {
            self.advance();
            let (segment, _) = self.expect_ident()?;
            segments.push(segment);
        }
        Ok(segments)
    }

    fn parse_identifier_list(&mut self) -> Result<Vec<String>, Diagnostic> {
        let mut items = Vec::new();
        if self.at_rbracket() {
            return Ok(items);
        }

        loop {
            let (ident, _) = self.expect_ident()?;
            items.push(ident);
            if self.at_comma() {
                self.advance();
                continue;
            }
            break;
        }

        Ok(items)
    }

    fn parse_string_list(&mut self) -> Result<Vec<String>, Diagnostic> {
        let mut items = Vec::new();
        if self.at_rbracket() {
            return Ok(items);
        }

        loop {
            let Some(value) = self.take_string() else {
                return Err(Diagnostic::error(
                    "expected string literal",
                    self.current().span,
                ));
            };
            items.push(value);
            if self.at_comma() {
                self.advance();
                continue;
            }
            break;
        }

        Ok(items)
    }

    fn parse_ensure_clause(&mut self) -> Result<EnsureClause, Diagnostic> {
        let left = self.parse_predicate_value()?;
        let op = self.parse_predicate_op()?;
        let right = self.parse_predicate_value()?;
        Ok(EnsureClause { left, op, right })
    }

    fn parse_predicate_value(&mut self) -> Result<PredicateValue, Diagnostic> {
        if let Some(value) = self.take_string() {
            return Ok(PredicateValue::String(value));
        }
        if let Some(value) = self.take_number() {
            return Ok(PredicateValue::Number(value));
        }
        if self.at_ident() || self.at_kw_output() || self.at_kw_state() {
            let mut segments = Vec::new();
            if self.at_kw_output() {
                self.expect_kw_output()?;
                segments.push("output".to_string());
            } else if self.at_kw_state() {
                self.expect_kw_state()?;
                segments.push("state".to_string());
            } else {
                let (head, _) = self.expect_ident()?;
                segments.push(head);
            }
            while self.at_dot() {
                self.advance();
                let (segment, _) = self.expect_ident()?;
                segments.push(segment);
            }
            return Ok(PredicateValue::Path(segments));
        }

        Err(Diagnostic::error(
            "expected predicate value",
            self.current().span,
        ))
    }

    fn parse_predicate_op(&mut self) -> Result<PredicateOp, Diagnostic> {
        let op = if self.at_eqeq() {
            PredicateOp::Eq
        } else if self.at_noteq() {
            PredicateOp::NotEq
        } else if self.at_lte() {
            PredicateOp::Lte
        } else if self.at_gte() {
            PredicateOp::Gte
        } else if self.at_langle() {
            PredicateOp::Lt
        } else if self.at_rangle() {
            PredicateOp::Gt
        } else if self.at_kw_in() {
            PredicateOp::In
        } else {
            return Err(Diagnostic::error(
                "expected predicate operator",
                self.current().span,
            ));
        };
        self.advance();
        Ok(op)
    }

    fn parse_type_ref(&mut self) -> Result<TypeRef, Diagnostic> {
        let (name, _) = self.expect_ident()?;
        let mut args = Vec::new();

        if self.at_langle() {
            self.expect_langle()?;
            if !self.at_rangle() {
                loop {
                    args.push(self.parse_type_arg()?);
                    if self.at_comma() {
                        self.advance();
                        continue;
                    }
                    break;
                }
            }
            self.expect_rangle()?;
        }

        Ok(TypeRef { name, args })
    }

    fn parse_type_arg(&mut self) -> Result<TypeArg, Diagnostic> {
        if let Some(value) = self.take_string() {
            return Ok(TypeArg::String(value));
        }
        if let Some(value) = self.take_number() {
            return Ok(TypeArg::Number(value));
        }
        if self.at_ident() {
            return Ok(TypeArg::Type(self.parse_type_ref()?));
        }

        Err(Diagnostic::error(
            "expected type argument",
            self.current().span,
        ))
    }

    fn expect_kw_cap(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_cap, "'cap'")
    }

    fn expect_kw_fn(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_fn, "'fn'")
    }

    fn expect_kw_agent(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_agent, "'agent'")
    }

    fn expect_kw_record(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_record, "'record'")
    }

    fn expect_kw_workflow(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_workflow, "'workflow'")
    }

    fn expect_kw_requires(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_requires, "'requires'")
    }

    fn expect_kw_intent(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_intent, "'intent'")
    }

    fn expect_kw_ensures(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_ensures, "'ensures'")
    }

    fn expect_kw_failure(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_failure, "'failure'")
    }

    fn expect_kw_evidence(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_evidence, "'evidence'")
    }

    fn expect_kw_trace(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_trace, "'trace'")
    }

    fn expect_kw_metrics(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_metrics, "'metrics'")
    }

    fn expect_kw_steps(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_steps, "'steps'")
    }

    fn expect_kw_on_fail(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_on_fail, "'on_fail'")
    }

    fn expect_kw_output(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_output, "'output'")
    }

    fn expect_kw_state(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_state, "'state'")
    }

    fn expect_kw_policy(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_policy, "'policy'")
    }

    fn expect_kw_loop(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_loop, "'loop'")
    }

    fn expect_kw_allow_tools(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_allow_tools, "'allow_tools'")
    }

    fn expect_kw_deny_tools(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_deny_tools, "'deny_tools'")
    }

    fn expect_kw_max_iterations(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_max_iterations, "'max_iterations'")
    }

    fn expect_kw_human_in_loop(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_human_in_loop, "'human_in_loop'")
    }

    fn expect_kw_stop(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_stop, "'stop'")
    }

    fn expect_kw_when(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_when, "'when'")
    }

    fn expect_kw_any(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_kw_any, "'any'")
    }

    fn expect_lparen(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_lparen, "'('")
    }

    fn expect_rparen(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_rparen, "')'")
    }

    fn expect_lbrace(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_lbrace, "'{'")
    }

    fn expect_rbrace(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_rbrace, "'}'")
    }

    fn expect_lbracket(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_lbracket, "'['")
    }

    fn expect_rbracket(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_rbracket, "']'")
    }

    fn expect_langle(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_langle, "'<'")
    }

    fn expect_rangle(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_rangle, "'>'")
    }

    fn expect_colon(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_colon, "':'")
    }

    fn expect_semicolon(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_semicolon, "';'")
    }

    fn expect_bang(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_bang, "'!'")
    }

    fn expect_eq(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_eq, "'='")
    }

    fn expect_arrow(&mut self) -> Result<Span, Diagnostic> {
        self.expect_simple(Self::at_arrow, "'->'")
    }

    fn expect_ident(&mut self) -> Result<(String, Span), Diagnostic> {
        let token = self.current();
        if let TokenKind::Ident(value) = &token.kind {
            let out = (value.clone(), token.span);
            self.advance();
            return Ok(out);
        }

        Err(Diagnostic::error(
            format!("expected identifier, found {}", self.current_kind_name()),
            token.span,
        ))
    }

    fn expect_simple(
        &mut self,
        predicate: fn(&Self) -> bool,
        expected: &str,
    ) -> Result<Span, Diagnostic> {
        if predicate(self) {
            let span = self.current().span;
            self.advance();
            return Ok(span);
        }

        Err(Diagnostic::error(
            format!("expected {expected}, found {}", self.current_kind_name()),
            self.current().span,
        ))
    }

    fn take_ident(&mut self) -> Option<String> {
        let token = self.current();
        if let TokenKind::Ident(value) = &token.kind {
            let value = value.clone();
            self.advance();
            return Some(value);
        }
        None
    }

    fn take_string(&mut self) -> Option<String> {
        let token = self.current();
        if let TokenKind::StringLiteral(value) = &token.kind {
            let value = value.clone();
            self.advance();
            return Some(value);
        }
        None
    }

    fn take_number(&mut self) -> Option<String> {
        let token = self.current();
        if let TokenKind::Number(value) = &token.kind {
            let value = value.clone();
            self.advance();
            return Some(value);
        }
        None
    }

    fn current(&self) -> &Token {
        &self.tokens[self.index]
    }

    fn at_eof(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    fn advance(&mut self) {
        if !self.at_eof() {
            self.index += 1;
        }
    }

    fn at_ident(&self) -> bool {
        matches!(self.current().kind, TokenKind::Ident(_))
    }

    fn at_kw_cap(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwCap)
    }

    fn at_kw_fn(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwFn)
    }

    fn at_kw_agent(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwAgent)
    }

    fn at_kw_record(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwRecord)
    }

    fn at_kw_workflow(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwWorkflow)
    }

    fn at_kw_requires(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwRequires)
    }

    fn at_kw_intent(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwIntent)
    }

    fn at_kw_ensures(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwEnsures)
    }

    fn at_kw_failure(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwFailure)
    }

    fn at_kw_evidence(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwEvidence)
    }

    fn at_kw_trace(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwTrace)
    }

    fn at_kw_metrics(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwMetrics)
    }

    fn at_kw_steps(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwSteps)
    }

    fn at_kw_on_fail(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwOnFail)
    }

    fn at_kw_output(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwOutput)
    }

    fn at_kw_state(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwState)
    }

    fn at_kw_policy(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwPolicy)
    }

    fn at_kw_loop(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwLoop)
    }

    fn at_kw_allow_tools(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwAllowTools)
    }

    fn at_kw_deny_tools(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwDenyTools)
    }

    fn at_kw_max_iterations(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwMaxIterations)
    }

    fn at_kw_human_in_loop(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwHumanInLoop)
    }

    fn at_kw_stop(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwStop)
    }

    fn at_kw_when(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwWhen)
    }

    fn at_kw_any(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwAny)
    }

    fn at_kw_in(&self) -> bool {
        matches!(self.current().kind, TokenKind::KwIn)
    }

    fn at_lparen(&self) -> bool {
        matches!(self.current().kind, TokenKind::LParen)
    }

    fn at_rparen(&self) -> bool {
        matches!(self.current().kind, TokenKind::RParen)
    }

    fn at_lbrace(&self) -> bool {
        matches!(self.current().kind, TokenKind::LBrace)
    }

    fn at_rbrace(&self) -> bool {
        matches!(self.current().kind, TokenKind::RBrace)
    }

    fn at_lbracket(&self) -> bool {
        matches!(self.current().kind, TokenKind::LBracket)
    }

    fn at_rbracket(&self) -> bool {
        matches!(self.current().kind, TokenKind::RBracket)
    }

    fn at_langle(&self) -> bool {
        matches!(self.current().kind, TokenKind::LAngle)
    }

    fn at_rangle(&self) -> bool {
        matches!(self.current().kind, TokenKind::RAngle)
    }

    fn at_comma(&self) -> bool {
        matches!(self.current().kind, TokenKind::Comma)
    }

    fn at_dot(&self) -> bool {
        matches!(self.current().kind, TokenKind::Dot)
    }

    fn at_colon(&self) -> bool {
        matches!(self.current().kind, TokenKind::Colon)
    }

    fn at_semicolon(&self) -> bool {
        matches!(self.current().kind, TokenKind::Semicolon)
    }

    fn at_bang(&self) -> bool {
        matches!(self.current().kind, TokenKind::Bang)
    }

    fn at_arrow(&self) -> bool {
        matches!(self.current().kind, TokenKind::Arrow)
    }

    fn at_eq(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eq)
    }

    fn at_eqeq(&self) -> bool {
        matches!(self.current().kind, TokenKind::EqEq)
    }

    fn at_noteq(&self) -> bool {
        matches!(self.current().kind, TokenKind::NotEq)
    }

    fn at_lte(&self) -> bool {
        matches!(self.current().kind, TokenKind::Lte)
    }

    fn at_gte(&self) -> bool {
        matches!(self.current().kind, TokenKind::Gte)
    }

    fn peek_kind_is_eq(&self) -> bool {
        matches!(
            self.tokens.get(self.index + 1).map(|token| &token.kind),
            Some(TokenKind::Eq)
        )
    }

    fn current_kind_name(&self) -> &'static str {
        match &self.current().kind {
            TokenKind::KwCap => "'cap'",
            TokenKind::KwFn => "'fn'",
            TokenKind::KwWorkflow => "'workflow'",
            TokenKind::KwAgent => "'agent'",
            TokenKind::KwRecord => "'record'",
            TokenKind::KwSteps => "'steps'",
            TokenKind::KwOnFail => "'on_fail'",
            TokenKind::KwOutput => "'output'",
            TokenKind::KwState => "'state'",
            TokenKind::KwPolicy => "'policy'",
            TokenKind::KwLoop => "'loop'",
            TokenKind::KwAllowTools => "'allow_tools'",
            TokenKind::KwDenyTools => "'deny_tools'",
            TokenKind::KwMaxIterations => "'max_iterations'",
            TokenKind::KwHumanInLoop => "'human_in_loop'",
            TokenKind::KwStop => "'stop'",
            TokenKind::KwWhen => "'when'",
            TokenKind::KwAny => "'any'",
            TokenKind::KwIntent => "'intent'",
            TokenKind::KwEnsures => "'ensures'",
            TokenKind::KwFailure => "'failure'",
            TokenKind::KwEvidence => "'evidence'",
            TokenKind::KwTrace => "'trace'",
            TokenKind::KwMetrics => "'metrics'",
            TokenKind::KwIn => "'in'",
            TokenKind::KwRequires => "'requires'",
            TokenKind::Ident(_) => "identifier",
            TokenKind::StringLiteral(_) => "string literal",
            TokenKind::Number(_) => "number",
            TokenKind::LParen => "'('",
            TokenKind::RParen => "')'",
            TokenKind::LBrace => "'{'",
            TokenKind::RBrace => "'}'",
            TokenKind::LBracket => "'['",
            TokenKind::RBracket => "']'",
            TokenKind::LAngle => "'<'",
            TokenKind::RAngle => "'>'",
            TokenKind::Comma => "','",
            TokenKind::Dot => "'.'",
            TokenKind::Colon => "':'",
            TokenKind::Semicolon => "';'",
            TokenKind::Bang => "'!'",
            TokenKind::Eq => "'='",
            TokenKind::EqEq => "'=='",
            TokenKind::NotEq => "'!='",
            TokenKind::Lte => "'<='",
            TokenKind::Gte => "'>='",
            TokenKind::Arrow => "'->'",
            TokenKind::Eof => "end of file",
        }
    }
}
