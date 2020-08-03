use tract_core::internal::*;

#[repr(C)]
#[derive(Debug)]
struct Header {
    magic: [u8; 2],
    version_maj: u8,
    version_min: u8,
    data_size_bytes: u32,
    rank: u32,
    dims: [u32; 8],
    bits_per_item: u32,
    item_type_vendor: u16,
    item_type: u16,
    item_type_params_deprecated: [u8; 32],
    padding: [u32; 11],
}

pub fn read_tensor<R: std::io::Read>(mut reader: R) -> TractResult<Tensor> {
    unsafe {
        let mut header: Header = std::mem::zeroed();
        let buffer: &mut [u8; 128] = std::mem::transmute(&mut header);
        reader.read_exact(buffer)?;
        if header.magic != [0x4e, 0xef] {
            bail!("Wrong magic number");
        }
        if header.version_maj != 1 && header.version_min != 0 {
            bail!("Wrong version number");
        }
        if header.rank > 8 {
            bail!("Wrong tensor rank {}", header.rank);
        }
        let shape: TVec<usize> =
            header.dims[0..header.rank as usize].iter().map(|d| *d as _).collect();
        let len = shape.iter().product::<usize>();
        if len * (header.bits_per_item as usize / 8) != header.data_size_bytes as usize {
            bail!(
                "Shape and len mismatch: shape:{:?}, bits_per_item:{}, bytes:{} ",
                shape,
                header.bits_per_item,
                header.data_size_bytes
            );
        }
        if header.item_type_vendor != 0 {
            bail!("Unknownn item type vendor {}", header.item_type_vendor);
        }
        let dt = match (header.item_type, header.bits_per_item) {
            (0, 16) => DatumType::F16,
            (0, 32) => DatumType::F32,
            (0, 64) => DatumType::F64,
            (1, 8) => DatumType::U8,
            (1, 16) => DatumType::U16,
            (1, 32) => DatumType::U32,
            (1, 64) => DatumType::U64,
            (0x0100, 8) => DatumType::I8,
            (0x0100, 16) => DatumType::I16,
            (0x0100, 32) => DatumType::I32,
            (0x0100, 64) => DatumType::I64,
            _ => bail!(
                "Unsupported type in tensor type:{} bits_per_item:{}",
                header.item_type,
                header.bits_per_item
            ),
        };
        let mut tensor = Tensor::uninitialized_dt(dt, &shape)?;
        let slice = std::slice::from_raw_parts_mut(
            tensor.as_ptr_mut_unchecked::<u8>(),
            header.data_size_bytes as usize,
        );
        reader.read_exact(slice)?;
        Ok(tensor)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn header_is_128_bytes() {
        assert_eq!(std::mem::size_of::<Header>(), 128);
    }
}
