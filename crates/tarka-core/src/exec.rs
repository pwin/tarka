//! Running the shape layer of a plan: one solution in, triples out.

use oxrdf::vocab::rdf;
use oxrdf::{BlankNode, NamedNode, NamedOrBlankNode, Term, Triple};

use crate::plan::{Block, Expander, Pattern, Plan, Repeat, TermPat, VarId};
use crate::value::Value;

/// Makes blank node labels. Labels are `{prefix}{n}`, so a backend that gives each
/// row its own prefix gets the same output whatever order rows are processed in.
#[derive(Clone, Debug)]
pub struct Labels {
    prefix: String,
    next: u64,
}

impl Labels {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self { prefix: prefix.into(), next: 0 }
    }

    /// Starts a new scope (for example a new row) with this prefix.
    pub fn reset(&mut self, prefix: impl Into<String>) {
        self.prefix = prefix.into();
        self.next = 0;
    }

    pub fn fresh(&mut self) -> BlankNode {
        self.next += 1;
        BlankNode::new_unchecked(format!("{}{}", self.prefix, self.next))
    }
}

/// Runs the shape layer of one plan.
pub struct Emitter<'p> {
    plan: &'p Plan,
    scope: Vec<Option<BlankNode>>,
}

impl<'p> Emitter<'p> {
    pub fn new(plan: &'p Plan) -> Self {
        Self { plan, scope: vec![None; plan.bnode_count] }
    }

    /// Emits the triples of one solution. `env` holds a value (or `None`) for each of
    /// the plan's variables; it is restored before returning.
    pub fn emit(&mut self, env: &mut [Option<Value>], labels: &mut Labels, out: &mut dyn FnMut(Triple)) {
        let root = &self.plan.root;
        self.block(root, env, labels, out);
    }

    fn block(&mut self, block: &Block, env: &mut [Option<Value>], labels: &mut Labels, out: &mut dyn FnMut(Triple)) {
        let saved: Vec<_> = block.bnodes.iter().map(|b| (b.0, self.scope[b.0].replace(labels.fresh()))).collect();
        for p in &block.patterns {
            self.pattern(p, env, labels, out);
        }
        for r in &block.repeats {
            self.repeat(r, env, labels, out);
        }
        for (i, previous) in saved {
            self.scope[i] = previous;
        }
    }

    fn pattern(&self, p: &Pattern, env: &[Option<Value>], labels: &mut Labels, out: &mut dyn FnMut(Triple)) {
        if !bound(&p.requires, env) {
            return;
        }
        let (Some(s), Some(pr), Some(o)) = (self.resolve(&p.subject, env), self.resolve(&p.predicate, env), self.resolve(&p.object, env))
        else {
            return;
        };
        let subject: NamedOrBlankNode = match s {
            Value::Term(Term::NamedNode(n)) => n.into(),
            Value::Term(Term::BlankNode(b)) => b.into(),
            _ => return, // a literal or a list cannot be a subject
        };
        let Value::Term(Term::NamedNode(predicate)) = pr else {
            return;
        };
        let object = match o {
            Value::Term(t) => t,
            Value::List(items) => collection(&items, labels, out),
        };
        out(Triple::new(subject, predicate, object));
    }

    fn repeat(&mut self, r: &Repeat, env: &mut [Option<Value>], labels: &mut Labels, out: &mut dyn FnMut(Triple)) {
        if !bound(&r.requires, env) {
            return;
        }
        let mut lists = Vec::with_capacity(r.lists.len());
        for (list, _) in &r.lists {
            match &env[list.0] {
                Some(Value::List(items)) => lists.push(items.clone()),
                Some(Value::Term(t)) => lists.push(vec![Value::Term(t.clone())]),
                None => return, // `none` for a list to expand: no instances
            }
        }
        let elements: Vec<VarId> = r.lists.iter().map(|(_, e)| *e).collect();
        for combo in combinations(r.expander, &lists) {
            let saved: Vec<_> = elements.iter().zip(combo).map(|(e, v)| std::mem::replace(&mut env[e.0], v)).collect();
            self.block(&r.body, env, labels, out);
            for (e, v) in elements.iter().zip(saved) {
                env[e.0] = v;
            }
        }
    }

    fn resolve(&self, t: &TermPat, env: &[Option<Value>]) -> Option<Value> {
        match t {
            TermPat::Const(c) => Some(Value::Term(c.clone())),
            TermPat::Var(v) => env[v.0].clone(),
            TermPat::BNode(b) => self.scope[b.0].clone().map(|b| Value::Term(b.into())),
            TermPat::Default(v, fallback) => env[v.0].clone().or_else(|| self.resolve(fallback, env)),
            TermPat::List(items) => items.iter().map(|i| self.resolve(i, env)).collect::<Option<Vec<_>>>().map(Value::List),
        }
    }
}

fn bound(vars: &[VarId], env: &[Option<Value>]) -> bool {
    vars.iter().all(|v| env[v.0].is_some())
}

/// Emits an RDF collection and returns its head.
fn collection(items: &[Value], labels: &mut Labels, out: &mut dyn FnMut(Triple)) -> Term {
    let nodes: Vec<BlankNode> = items.iter().map(|_| labels.fresh()).collect();
    for (i, (node, item)) in nodes.iter().zip(items).enumerate() {
        let first = match item {
            Value::Term(t) => t.clone(),
            Value::List(inner) => collection(inner, labels, out),
        };
        out(Triple::new(node.clone(), rdf::FIRST, first));
        let rest: Term = match nodes.get(i + 1) {
            Some(next) => next.clone().into(),
            None => NamedNode::from(rdf::NIL).into(),
        };
        out(Triple::new(node.clone(), rdf::REST, rest));
    }
    match nodes.first() {
        Some(head) => head.clone().into(),
        None => NamedNode::from(rdf::NIL).into(),
    }
}

/// The element tuples an expander runs over. `None` pads the shorter lists in `zipMax`.
fn combinations(expander: Expander, lists: &[Vec<Value>]) -> Vec<Vec<Option<Value>>> {
    match expander {
        Expander::Cross => {
            let mut out: Vec<Vec<Option<Value>>> = vec![Vec::new()];
            for list in lists {
                out = out
                    .into_iter()
                    .flat_map(|prefix| {
                        list.iter().map(move |v| {
                            let mut next = prefix.clone();
                            next.push(Some(v.clone()));
                            next
                        })
                    })
                    .collect();
            }
            out
        }
        Expander::ZipMin | Expander::ZipMax => {
            let lengths = lists.iter().map(Vec::len);
            let n = if expander == Expander::ZipMin { lengths.min() } else { lengths.max() }.unwrap_or(0);
            (0..n).map(|i| lists.iter().map(|l| l.get(i).cloned()).collect()).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{Lifting, Pattern};
    use oxrdf::{Literal, NamedNode};

    fn iri(s: &str) -> Term {
        NamedNode::new_unchecked(format!("http://ex/{s}")).into()
    }

    fn run(plan: &Plan, env: &mut [Option<Value>]) -> Vec<String> {
        let mut out = Vec::new();
        Emitter::new(plan).emit(env, &mut Labels::new("b"), &mut |t| out.push(t.to_string()));
        out
    }

    #[test]
    fn requirements_drop_patterns() {
        let mut plan = Plan::new("t", Lifting::Given);
        let (x, y) = (plan.var("x"), plan.var("y"));
        let node = plan.bnode();
        plan.root.bnodes.push(node);
        plan.root.patterns.push(Pattern {
            subject: TermPat::Var(x),
            predicate: TermPat::Const(iri("p")),
            object: TermPat::BNode(node),
            requires: vec![y],
        });
        plan.root.patterns.push(Pattern {
            subject: TermPat::BNode(node),
            predicate: TermPat::Const(iri("q")),
            object: TermPat::Default(y, Box::new(TermPat::Const(Literal::new_simple_literal("d").into()))),
            requires: vec![],
        });
        let mut env = vec![Some(Value::Term(iri("s"))), None];
        assert_eq!(run(&plan, &mut env), vec![r#"_:b1 <http://ex/q> "d""#]);
        env[1] = Some(Value::Term(Literal::new_simple_literal("v").into()));
        assert_eq!(run(&plan, &mut env), vec![r#"<http://ex/s> <http://ex/p> _:b1"#, r#"_:b1 <http://ex/q> "v""#]);
    }

    #[test]
    fn repeats_and_collections() {
        let mut plan = Plan::new("t", Lifting::Given);
        let (s, list, item) = (plan.var("s"), plan.var("list"), plan.var("item"));
        let node = plan.bnode();
        let mut body = Block::default();
        body.bnodes.push(node);
        body.patterns.push(Pattern {
            subject: TermPat::Var(s),
            predicate: TermPat::Const(iri("has")),
            object: TermPat::BNode(node),
            requires: vec![],
        });
        body.patterns.push(Pattern {
            subject: TermPat::BNode(node),
            predicate: TermPat::Const(iri("value")),
            object: TermPat::Var(item),
            requires: vec![],
        });
        plan.root.repeats.push(Repeat { expander: Expander::Cross, lists: vec![(list, item)], requires: vec![], body });
        plan.root.patterns.push(Pattern {
            subject: TermPat::Var(s),
            predicate: TermPat::Const(iri("all")),
            object: TermPat::Var(list),
            requires: vec![],
        });
        let lit = |v: &str| Value::Term(Literal::new_simple_literal(v).into());
        let mut env = vec![Some(Value::Term(iri("s"))), Some(Value::List(vec![lit("a"), lit("b")])), None];
        let out = run(&plan, &mut env);
        assert_eq!(out.len(), 4 + 4 + 1, "{out:#?}");
        // root patterns run before repeats: the collection gets b1, b2 and each element a fresh node
        assert!(out.contains(&r#"<http://ex/s> <http://ex/all> _:b1"#.to_owned()));
        assert!(out.contains(
            &r#"_:b2 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil>"#.to_owned()
        ));
        assert!(out.contains(&r#"_:b3 <http://ex/value> "a""#.to_owned()));
        assert!(out.contains(&r#"_:b4 <http://ex/value> "b""#.to_owned()));
        assert_eq!(env[2], None, "element variable restored");
    }

    #[test]
    fn zips() {
        let l = |n: usize| (0..n).map(|i| Value::Term(Literal::from(i as i64).into())).collect::<Vec<_>>();
        assert_eq!(combinations(Expander::ZipMin, &[l(2), l(3)]).len(), 2);
        let max = combinations(Expander::ZipMax, &[l(2), l(3)]);
        assert_eq!(max.len(), 3);
        assert_eq!(max[2][0], None);
        assert_eq!(combinations(Expander::Cross, &[l(2), l(3)]).len(), 6);
        assert_eq!(combinations(Expander::Cross, &[l(2), l(0)]).len(), 0);
    }
}
