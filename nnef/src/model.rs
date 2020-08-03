use crate::ast::*;
use std::collections::HashMap;
use tract_core::internal::*;

use crate::primitives::Primitives;

pub struct Framework {
    stdlib: Vec<Arc<FragmentDef>>,
    primitives: Primitives,
}

impl Framework {
    fn new() -> Framework {
        Framework {
            stdlib: crate::parser::parse_fragments(include_str!("../stdlib.nnef"))
                .unwrap()
                .into_iter()
                .map(Arc::new)
                .collect(),
            primitives: crate::primitives::primitives(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProtoModel {
    pub doc: Document,
}

impl ProtoModel {
    pub fn into_typed_model(&self) -> TractResult<TypedModel> {
        let framework = Framework::new();
        let mut builder = ModelBuilder {
            framework,
            fragments: self.doc.fragments.iter().map(|f| Arc::new(f.clone())).collect(),
            model: TypedModel::default(),
            naming_scopes: vec![],
            scopes: vec![],
        };
        builder.scopes.push(HashMap::new());
        builder.naming_scopes.push(self.doc.graph_def.id.to_string());
        builder
            .wire_body(&self.doc.graph_def.body)
            .chain_err(|| format!("Mapping graph `{}' to tract", self.doc.graph_def.id))?;
        let vars = builder.scopes.pop().unwrap();
        let outputs = self
            .doc
            .graph_def
            .results
            .iter()
            .map(|s| vars[s].to::<OutletId>(&mut builder))
            .collect::<TractResult<TVec<OutletId>>>()?;
        builder.model.set_output_outlets(&outputs)?;
        Ok(builder.model)
    }
}

#[derive(Clone, Debug)]
pub enum Value {
    Tensor(Arc<Tensor>),
    Wire(OutletId),
    Array(Vec<Value>),
    Tuple(Vec<Value>),
    String(String),
    Bool(bool),
    Scalar(f32),
    Dim(TDim),
}

impl Value {
    pub fn to<T>(&self, builder: &mut ModelBuilder) -> TractResult<T>
    where
        T: CoerceFrom<Value>,
    {
        T::coerce(builder, self)
    }
}

pub struct ModelBuilder {
    pub framework: Framework,
    pub fragments: Vec<Arc<FragmentDef>>,
    pub model: TypedModel,
    pub naming_scopes: Vec<String>,
    pub scopes: Vec<HashMap<String, Value>>,
}

impl ModelBuilder {
    pub fn wire(
        &mut self,
        op: impl Into<Box<dyn TypedOp>>,
        inputs: &[OutletId],
    ) -> TractResult<TVec<OutletId>> {
        let op = op.into();
        self.model.wire_node(
            format!("{}.{}", self.naming_scopes.join("."), op.as_op().name()),
            op,
            inputs,
        )
    }
}

#[derive(Clone, Debug)]
pub struct AugmentedInvocation<'a> {
    pub invocation: &'a Invocation,
    pub fragment: Arc<FragmentDef>,
}

impl<'a> AugmentedInvocation<'a> {
    pub fn named_arg_as<T>(&self, builder: &mut ModelBuilder, name: &str) -> TractResult<T>
    where
        T: CoerceFrom<Value>,
    {
        let rv = self.named_arg(name)?;
        let v = rv
            .resolve(builder)
            .chain_err(|| format!("Resolving argument `{}' ({:?})", name, rv))?;
        v.to::<T>(builder).chain_err(|| format!("Converting argument `{}' from {:?}", name, v))
    }

    pub fn named_arg(&self, name: &str) -> TractResult<Cow<RValue>> {
        self.get_named_arg(name).ok_or_else(|| format!("expected argument {}", name).into())
    }

    pub fn get_named_arg(&self, name: &str) -> Option<Cow<RValue>> {
        // first look explicit name in invocation arguments
        if let Some(arg) =
            self.invocation.arguments.iter().find(|arg| arg.id.as_deref() == Some(name))
        {
            return Some(Cow::Borrowed(&arg.rvalue));
        }
        // then use fragment prototype:
        if let Some((ix, param)) =
            self.fragment.decl.parameters.iter().enumerate().find(|(_ix, param)| param.id == name)
        {
            // check that all previous (and our) arguments are positional (todo:
            // valid args when building augmented_invocation)
            if self.invocation.arguments.len() > ix
                && self.invocation.arguments.iter().take(ix + 1).all(|arg| arg.id.is_none())
            {
                return Some(Cow::Borrowed(&self.invocation.arguments[ix].rvalue));
            }
            if let Some(rv) = &param.lit {
                return Some(Cow::Owned(RValue::Literal(rv.clone())));
            }
        }
        None
    }
}

impl ModelBuilder {
    fn augmented_invocation<'a>(
        &self,
        invocation: &'a Invocation,
    ) -> TractResult<AugmentedInvocation<'a>> {
        let fragment = self
            .framework
            .stdlib
            .iter()
            .find(|f| f.decl.id == invocation.id)
            .cloned()
            .or_else(|| self.fragments.iter().find(|f| f.decl.id == invocation.id).cloned())
            .ok_or_else(|| format!("No fragment definition found for `{}'", invocation.id))?;
        Ok(AugmentedInvocation { invocation, fragment })
    }

    pub fn wire_body(&mut self, body: &[Assignment]) -> TractResult<()> {
        // todo: can i relax the outlet id constraint ?
        for assignment in body {
            let identifiers = assignment.left.to_identifiers()?;
            self.naming_scopes.push(identifiers[0].to_string());
            let values: TVec<OutletId> =
                assignment.right.resolve(self).and_then(|v| v.to(self)).chain_err(|| {
                    format!("Plugging in assignement for {:?}", identifiers.join(", "))
                })?;
            if values.len() != identifiers.len() {
                bail!(
                    "Assignement for {} received {} value(s).",
                    identifiers.join(","),
                    values.len()
                )
            }
            self.model.node_mut(values[0].node).name = format!("{}", self.naming_scopes.join("."));
            for (id, outlet) in identifiers.iter().zip(values.iter()) {
                self.scopes.last_mut().unwrap().insert(id.to_string(), Value::Wire(*outlet));
            }
            self.naming_scopes.pop();
        }
        Ok(())
    }

    pub fn wire_invocation(&mut self, invocation: &Invocation) -> TractResult<Value> {
        let augmented_invocation = self.augmented_invocation(invocation)?;
        if let Some(prim) = self.framework.primitives.get(&invocation.id).cloned() {
            (prim)(self, &augmented_invocation)
                .map(|res| Value::Tuple(res.into_iter().map(Value::Wire).collect()))
                .chain_err(|| format!("Plugging primitive `{}'", invocation.id))
        } else if augmented_invocation.fragment.body.is_some() {
            self.wire_fragment_invocation(&augmented_invocation)
                .chain_err(|| format!("Expanding fragment `{}'", invocation.id))
        } else {
            bail!(
                "fragment for `{}' is declarative, not defining. Maybe a missing primitive ?",
                invocation.id
            )
        }
    }

    pub fn wire_fragment_invocation(
        &mut self,
        invocation: &AugmentedInvocation,
    ) -> TractResult<Value> {
        let mut inner_scope = HashMap::new();
        for par in invocation.fragment.decl.parameters.iter() {
            inner_scope
                .insert(par.id.to_string(), invocation.named_arg_as::<Value>(self, &par.id)?);
        }
        self.scopes.push(inner_scope);
        self.naming_scopes.push(invocation.invocation.id.to_string());
        self.wire_body(&invocation.fragment.body.as_ref().unwrap())?;
        self.naming_scopes.pop();
        let inner_scope = self.scopes.pop().unwrap();
        Ok(Value::Tuple(
            invocation
                .fragment
                .decl
                .results
                .iter()
                .map(|res| inner_scope.get(&res.id).unwrap())
                .cloned()
                .collect(),
        ))
    }
}

impl LValue {
    fn to_identifier(&self) -> TractResult<&str> {
        match self {
            LValue::Identifier(id) => Ok(&**id),
            _ => bail!("Expected an identifier, found a tuple: {:?}", self),
        }
    }

    #[allow(dead_code)]
    fn to_identifiers(&self) -> TractResult<TVec<&str>> {
        match self {
            LValue::Identifier(_) => Ok(tvec!(self.to_identifier()?)),
            LValue::Tuple(ids) => ids.iter().map(|id| id.to_identifier()).collect(),
            LValue::Array(ids) => ids.iter().map(|id| id.to_identifier()).collect(),
        }
    }
}

impl Invocation {}

impl RValue {
    pub fn resolve(&self, builder: &mut ModelBuilder) -> TractResult<Value> {
        match self {
            RValue::Identifier(id) => {
                let outlet = builder
                    .scopes
                    .last()
                    .unwrap()
                    .get(id)
                    .cloned()
                    .ok_or_else(|| format!("No value for name {}", id))?;
                Ok(outlet)
            }
            RValue::Invocation(inv) => builder.wire_invocation(inv),
            RValue::Binary(left, op, right) => {
                let op = match &**op {
                    "+" => "add",
                    "-" => "sub",
                    "*" => "mul",
                    "/" => "div",
                    "^" => "pow",
                    ">" => "gt",
                    "<" => "lt",
                    "==" => "eq",
                    "!=" => "ne",
                    ">=" => "ge",
                    "<=" => "le",
                    op => bail!("Unknown binary operator: {}", op),
                };
                let inv = Invocation {
                    id: op.to_string(),
                    generic_type_name: None,
                    arguments: vec![
                        Argument { id: None, rvalue: left.as_ref().clone() },
                        Argument { id: None, rvalue: right.as_ref().clone() },
                    ],
                };
                builder.wire_invocation(&inv)
            }
            RValue::Array(array) => Ok(Value::Array(
                array.iter().map(|i| i.resolve(builder)).collect::<TractResult<_>>()?,
            )),
            RValue::Tuple(array) => Ok(Value::Tuple(
                array.iter().map(|i| i.resolve(builder)).collect::<TractResult<_>>()?,
            )),
            RValue::Literal(Literal::Numeric(f)) => {
                if f.0.contains(".") || f.0.contains("e") {
                    f.0.parse::<f32>()
                        .map(Value::Scalar)
                        .map_err(|_| format!("Can not parse {} as f32", f.0).into())
                } else {
                    f.0.parse::<TDim>()
                        .map(Value::Dim)
                        .map_err(|_| format!("Can not parse {} as i64", f.0).into())
                }
            }
            RValue::Literal(Literal::String(StringLiteral(s))) => Ok(Value::String(s.clone())),
            RValue::Literal(Literal::Logical(LogicalLiteral(s))) => Ok(Value::Bool(*s)),
            RValue::Literal(Literal::Array(array)) => Ok(Value::Array(
                array.iter().map(|i| RValue::Literal(i.clone()).resolve(builder)).collect::<TractResult<_>>()?,
            )),
            _ => panic!("{:?}", self),
        }
    }
}

/*
   pub fn to_wire(&self, builder: &mut ModelBuilder) -> TractResult<OutletId> {
   let wires = self.to_wires(builder)?;
   if wires.len() != 1 {
   bail!("Expected 1 wire, got {:?}", wires);
   }
   Ok(wires[0])
   }

   pub fn to_wires(&self, builder: &mut ModelBuilder) -> TractResult<TVec<OutletId>> {
   self.to_wires_rec(builder, true)
   }

   fn to_wires_rec(
   &self,
   builder: &mut ModelBuilder,
   can_try_const: bool,
   ) -> TractResult<TVec<OutletId>> {
   match self {
   RValue::Identifier(id) => {
   let outlet = builder
   .scopes
   .last()
   .unwrap()
   .get(id)
   .cloned()
   .ok_or_else(|| format!("No value for name {}", id))?;
   Ok(tvec!(outlet))
   }
   _ if can_try_const => {
   let tensor = self.to_tensor(builder)?;
   Ok(tvec!(builder.model.add_const("", tensor)?))
   }
   _ => bail!("failed to wire {:?}", self),
   }
   }

   pub fn to_tensor(&self, builder: &mut ModelBuilder) -> TractResult<Arc<Tensor>> {
   match self {
   RValue::Literal(Literal::Array(array)) => {
   if array.len() == 0 {
   return Ok(rctensor1::<i64>(&[]));
   }
   todo!()
   }
   RValue::Literal(Literal::Logical(LogicalLiteral(b))) => Ok(rctensor0(*b)),
   RValue::Literal(Literal::Numeric(f)) => {
   if f.0.contains(".") || f.0.contains("e") {
   f.0.parse::<f32>()
   .map(rctensor0)
   .map_err(|_| format!("Can not parse {} as f32", f.0).into())
   } else {
   f.0.parse::<i64>()
   .map(rctensor0)
   .map_err(|_| format!("Can not parse {} as i64", f.0).into())
   }
   }
   RValue::Literal(Literal::String(StringLiteral(s))) => Ok(rctensor0(s.to_owned())),
   RValue::Array(array) | RValue::Tuple(array) => {
   if array.len() == 0 {
   return Ok(rctensor1::<i64>(&[]));
   }
   let values: Vec<Arc<Tensor>> = array
   .iter()
   .map(|item| item.to_tensor(builder))
   .collect::<TractResult<Vec<Arc<Tensor>>>>()?;
   let values: Vec<Tensor> = values
   .into_iter()
   .map(|t| {
   let mut t = t.into_tensor();
   t.insert_axis(0)?;
Ok(t)
    })
.collect::<TractResult<Vec<_>>>()?;
Tensor::stack_tensors(0, &values).map(|t| t.into_arc_tensor())
    }
_ => {
    let wire = self
        .to_wires_rec(builder, false)
        .chain_err(|| "Failed to get a tensor, trying an wire instead.")?[0];
    builder.model.outlet_fact(wire)?.konst.clone().ok_or("Not a constant".into())
}
}
}

/*
   pub fn to_scalar<D: Datum>(&self, builder: &mut ModelBuilder) -> TractResult<D> {
   let d = self.to_tensor(builder)?;
   Ok(d.to_scalar::<D>()?.clone())
   }
   */

pub fn to_usizes(&self, builder: &mut ModelBuilder) -> TractResult<TVec<usize>> {
    let shape = self.to_tensor(builder)?;
    let shape = shape.cast_to::<i64>()?;
    Ok(shape.as_slice::<i64>()?.iter().map(|d| *d as usize).collect())
}

pub fn to_dims(&self, builder: &mut ModelBuilder) -> TractResult<TVec<TDim>> {
    let shape = self.to_tensor(builder)?;
    let shape = shape.cast_to::<TDim>()?;
    Ok(shape.as_slice::<TDim>()?.iter().cloned().collect())
}

pub fn to_shape_fact(&self, builder: &mut ModelBuilder) -> TractResult<ShapeFact> {
    let shape = self.to_tensor(builder)?;
    let shape = shape.cast_to::<TDim>()?;
    if shape.rank() != 1 {
        bail!("Shape are expected to be vectors (1D tensor) found: {:?}")
    }
    ShapeFact::from_dims(shape.as_slice::<TDim>()?)
}

pub fn to_dim(&self) -> TractResult<TDim> {
    self.as_literal()
        .map(|l| l.to_dim())
        .transpose()?
        .ok_or_else(|| format!("Expected {:?} to be a dim", self).into())
}

pub fn as_array(&self) -> Option<&[RValue]> {
    match self {
        RValue::Array(values) => Some(values),
        _ => None,
    }
}

pub fn as_literal(&self) -> Option<&Literal> {
    match self {
        RValue::Literal(lit) => Some(lit),
        _ => None,
    }
}
}
*/

pub trait CoerceFrom<F> {
    fn coerce(builder: &mut ModelBuilder, from: &F) -> TractResult<Self>
    where
        Self: Sized;
}

impl CoerceFrom<Value> for Value {
    fn coerce(builder: &mut ModelBuilder, from: &Value) -> TractResult<Self> {
        Ok(from.clone())
    }
}

impl CoerceFrom<Value> for Arc<Tensor> {
    fn coerce(builder: &mut ModelBuilder, from: &Value) -> TractResult<Self> {
        match from {
            Value::Tensor(t) => Ok(t.clone()),
            Value::Scalar(f) => Ok(rctensor0(*f)),
            Value::Wire(o) => {
                builder.model.outlet_fact(*o)?.konst.clone().ok_or_else(|| "Not a const".into())
            }
            _ => bail!("Can not build a tensor from {:?}", from),
        }
    }
}

impl CoerceFrom<Value> for OutletId {
    fn coerce(builder: &mut ModelBuilder, from: &Value) -> TractResult<Self> {
        match from {
            Value::Scalar(f) => {
                Ok(builder.wire(tract_core::ops::konst::Const::new(rctensor0(*f)), &[])?[0])
            }
            Value::Wire(outlet) => Ok(*outlet),
            Value::Tuple(tuple) if tuple.len() == 1 => OutletId::coerce(builder, &tuple[0]),
            _ => bail!("Can not build an outletid from {:?}", from),
        }
    }
}

impl CoerceFrom<Value> for i64 {
    fn coerce(builder: &mut ModelBuilder, from: &Value) -> TractResult<Self> {
        match from {
            Value::Dim(d) => d.to_integer().map(|d| d as _),
            _ => bail!("Can not build a i64 from {:?}", from),
        }
        // Arc::<Tensor>::coerce(builder, from)?.cast_to_scalar::<i64>()
    }
}

impl CoerceFrom<Value> for String {
    fn coerce(builder: &mut ModelBuilder, from: &Value) -> TractResult<Self> {
        match from {
            Value::String(s) => Ok(s.to_string()),
            Value::Tensor(t) => Ok(t.to_scalar::<String>()?.clone()),
            _ => bail!("Can not build a String from {:?}", from),
        }
    }
}

impl CoerceFrom<Value> for bool {
    fn coerce(builder: &mut ModelBuilder, from: &Value) -> TractResult<Self> {
        if let Value::Bool(b) = from {
            Ok(*b)
        } else {
            bail!("Can not build a boolean from {:?}", from)
        }
    }
}

impl CoerceFrom<Value> for usize {
    fn coerce(builder: &mut ModelBuilder, from: &Value) -> TractResult<Self> {
        Ok(i64::coerce(builder, from)? as usize)
    }
}

impl<D: CoerceFrom<Value>> CoerceFrom<Value> for TVec<D> {
    fn coerce(builder: &mut ModelBuilder, from: &Value) -> TractResult<Self> {
        match from {
            Value::Array(vec) => vec.iter().map(|item| D::coerce(builder, item)).collect(),
            Value::Tuple(vec) => vec.iter().map(|item| D::coerce(builder, item)).collect(),
            _ => bail!("Can not build an array from {:?}", from),
        }
    }
}

impl Literal {
    fn to_dim(&self) -> TractResult<TDim> {
        match self {
            Literal::Numeric(d) => Ok(d.0.parse::<i64>()?.to_dim()),
            _ => bail!("not a dimension"),
        }
    }
}
