use crate::pb::*;
use std::convert::{TryFrom, TryInto};
use tract_core::internal::*;
use tract_core::*;

impl TryFrom<TensorProto_DataType> for DatumType {
    type Error = TractError;
    fn try_from(t: TensorProto_DataType) -> TractResult<DatumType> {
        use self::TensorProto_DataType::*;
        match t {
            BOOL => Ok(DatumType::Bool),
            UINT8 => Ok(DatumType::U8),
            UINT16 => Ok(DatumType::U16),
            INT8 => Ok(DatumType::I8),
            INT16 => Ok(DatumType::I16),
            INT32 => Ok(DatumType::I32),
            INT64 => Ok(DatumType::I64),
            FLOAT16 => Ok(DatumType::F16),
            FLOAT => Ok(DatumType::F32),
            DOUBLE => Ok(DatumType::F64),
            STRING => Ok(DatumType::String),
            _ => Err(format!("Unknown DatumType {:?}", t))?,
        }
    }

    /*
    fn to_onnx(&self) -> TractResult<TensorProto_DataType> {
        use self::TensorProto_DataType::*;
        match self {
            DatumType::U8 => Ok(UINT8),
            DatumType::I8 => Ok(INT8),
            DatumType::I32 => Ok(INT32),
            DatumType::F32 => Ok(FLOAT),
            DatumType::F64 => Ok(DOUBLE),
            DatumType::String => Ok(STRING),
            DatumType::TDim => bail!("Dimension is not translatable in protobuf"),
        }
    }
    */
}

impl<'a> TryFrom<&'a TypeProto_Tensor> for InferenceFact {
    type Error = TractError;
    fn try_from(t: &'a TypeProto_Tensor) -> TractResult<InferenceFact> {
        let mut fact = InferenceFact::default();
        if t.has_elem_type() {
            fact = fact.with_datum_type(t.get_elem_type().try_into()?);
        }
        if t.has_shape() {
            let shape = t.get_shape();
            let shape: TVec<DimFact> = shape
                .get_dim()
                .iter()
                .map(|d| {
                    if d.get_dim_value() as usize == 0 {
                        DimFact::default()
                    } else {
                        DimFact::from(d.get_dim_value().to_dim())
                    }
                })
                .collect();
            fact = fact.with_shape(ShapeFact::closed(shape));
        }
        Ok(fact)
    }
}

impl TryFrom<TypeProto_Tensor> for InferenceFact {
    type Error = TractError;
    fn try_from(t: TypeProto_Tensor) -> TractResult<InferenceFact> {
        (&t).try_into()
    }
}

impl<'a> TryFrom<&'a TensorProto> for Tensor {
    type Error = TractError;
    fn try_from(t: &TensorProto) -> TractResult<Tensor> {
        let dt = t.get_data_type().try_into()?;
        let shape: Vec<usize> = t.get_dims().iter().map(|&i| i as usize).collect();
        if t.has_raw_data() {
            unsafe {
                match dt {
                    DatumType::U8 => Tensor::from_raw::<u8>(&*shape, t.get_raw_data()),
                    DatumType::U16 => Tensor::from_raw::<u16>(&*shape, t.get_raw_data()),
                    DatumType::I8 => Tensor::from_raw::<i8>(&*shape, t.get_raw_data()),
                    DatumType::I16 => Tensor::from_raw::<i16>(&*shape, t.get_raw_data()),
                    DatumType::I32 => Tensor::from_raw::<i32>(&*shape, t.get_raw_data()),
                    DatumType::I64 => Tensor::from_raw::<i64>(&*shape, t.get_raw_data()),
                    DatumType::F16 => Tensor::from_raw::<f16>(&*shape, t.get_raw_data()),
                    DatumType::F32 => Tensor::from_raw::<f32>(&*shape, t.get_raw_data()),
                    DatumType::F64 => Tensor::from_raw::<f64>(&*shape, t.get_raw_data()),
                    DatumType::Bool => Ok(Tensor::from_raw::<u8>(&*shape, t.get_raw_data())?
                        .into_array::<u8>()?
                        .mapv(|x| x != 0)
                        .into()),
                    _ => unimplemented!("FIXME, raw tensor loading"),
                }
            }
        } else {
            use ndarray::Array;
            let it = match dt {
                DatumType::Bool => Array::from_shape_vec(
                    &*shape,
                    t.get_int32_data().iter().map(|&x| x != 0).collect(),
                )?
                .into(),
                DatumType::U8 => Array::from_shape_vec(
                    &*shape,
                    t.get_int32_data().iter().map(|&x| x as u8).collect(),
                )?
                .into(),
                DatumType::U16 => Array::from_shape_vec(
                    &*shape,
                    t.get_int32_data().iter().map(|&x| x as u16).collect(),
                )?
                .into(),
                DatumType::I8 => Array::from_shape_vec(
                    &*shape,
                    t.get_int32_data().iter().map(|&x| x as i8).collect(),
                )?
                .into(),
                DatumType::I16 => Array::from_shape_vec(
                    &*shape,
                    t.get_int32_data().iter().map(|&x| x as i16).collect(),
                )?
                .into(),
                DatumType::I32 => {
                    Array::from_shape_vec(&*shape, t.get_int32_data().to_vec())?.into()
                }
                DatumType::I64 => {
                    Array::from_shape_vec(&*shape, t.get_int64_data().to_vec())?.into()
                }
                DatumType::F32 => {
                    Array::from_shape_vec(&*shape, t.get_float_data().to_vec())?.into()
                }
                DatumType::F64 => {
                    Array::from_shape_vec(&*shape, t.get_double_data().to_vec())?.into()
                }
                DatumType::String => {
                    let strings = t
                        .get_string_data()
                        .iter()
                        .cloned()
                        .map(String::from_utf8)
                        .collect::<Result<Vec<String>, _>>()
                        .map_err(|_| format!("Invalid UTF8 buffer"))?;
                    Array::from_shape_vec(&*shape, strings)?.into()
                }
                _ => unimplemented!("FIXME, struct tensor loading"),
            };
            Ok(it)
        }
    }
}

impl TryFrom<TensorProto> for Tensor {
    type Error = TractError;
    fn try_from(t: TensorProto) -> TractResult<Tensor> {
        (&t).try_into()
    }
}

pub fn proto_from_reader<R: ::std::io::Read>(mut r: R) -> TractResult<TensorProto> {
    protobuf::parse_from_reader(&mut r)
        .map_err(|e| format!("Can not parse protobuf input: {:?}", e).into())
}

pub fn from_reader<R: ::std::io::Read>(r: R) -> TractResult<Tensor> {
    proto_from_reader(r)?.try_into()
}
