use num_traits::{AsPrimitive, Zero};
use std::marker::PhantomData;
use std::{fmt, ops};

use crate::frame::mmm::LinearSpec::*;
use crate::frame::mmm::PanelStore::*;
use crate::frame::mmm::*;

#[derive(Copy, Clone, Debug)]
pub struct GenericMmm4x4<TA, TB, TC, TI>(PhantomData<(TA, TB, TC, TI)>);

unsafe impl<TA, TB, TC, TI> Send for GenericMmm4x4<TA, TB, TC, TI> {}
unsafe impl<TA, TB, TC, TI> Sync for GenericMmm4x4<TA, TB, TC, TI> {}

impl<TA, TB, TC, TI> MatMatMulKer<TA, TB, TC, TI> for GenericMmm4x4<TA, TB, TC, TI>
where
    TA: Copy + fmt::Debug + AsPrimitive<TI>,
    TB: Copy + fmt::Debug + AsPrimitive<TI>,
    TC: Copy + fmt::Debug + AsPrimitive<TI> + 'static,
    TI: Copy
        + ops::AddAssign
        + ops::SubAssign
        + ops::Mul<Output = TI>
        + ops::MulAssign
        + PartialOrd
        + Zero
        + fmt::Debug
        + fmt::Display
        + AsPrimitive<TC>
        + 'static,
{
    #[inline(always)]
    fn name() -> &'static str {
        "generic"
    }
    #[inline(always)]
    fn mr() -> usize {
        4
    }
    #[inline(always)]
    fn nr() -> usize {
        4
    }
    #[inline(always)]
    fn alignment_bytes_packed_a() -> usize {
        std::mem::size_of::<TA>()
    }
    #[inline(always)]
    fn alignment_bytes_packed_b() -> usize {
        std::mem::size_of::<TB>()
    }
    #[inline(never)]
    fn kernel(spec: &MatMatMulKerSpec<TA, TB, TC, TI>) -> isize {
        unsafe {
            let mut ab = [[TI::zero(); 4]; 4];
            if let Mul { k, zero_point_a, zero_point_b } = *spec.linear {
                match (*spec.a, *spec.b) {
                    (Packed { ptr: pa }, Packed { ptr: pb }) => {
                        for i in 0..k {
                            let mut a = [TI::zero(); 4];
                            let mut b = [TI::zero(); 4];
                            for ix in 0..4 {
                                a[ix] = (*pa.offset(((4 * i) + ix) as isize)).as_();
                                if !zero_point_a.is_null() {
                                    a[ix] -= *zero_point_a.offset(ix as isize);
                                }
                            }
                            for ix in 0..4 {
                                b[ix] = (*pb.offset(((4 * i) + ix) as isize)).as_();
                                if !zero_point_b.is_null() {
                                    b[ix] -= *zero_point_b.offset(ix as isize);
                                }
                            }
                            ab[0][0] += a[0] * b[0];
                            ab[0][1] += a[0] * b[1];
                            ab[0][2] += a[0] * b[2];
                            ab[0][3] += a[0] * b[3];
                            ab[1][0] += a[1] * b[0];
                            ab[1][1] += a[1] * b[1];
                            ab[1][2] += a[1] * b[2];
                            ab[1][3] += a[1] * b[3];
                            ab[2][0] += a[2] * b[0];
                            ab[2][1] += a[2] * b[1];
                            ab[2][2] += a[2] * b[2];
                            ab[2][3] += a[2] * b[3];
                            ab[3][0] += a[3] * b[0];
                            ab[3][1] += a[3] * b[1];
                            ab[3][2] += a[3] * b[2];
                            ab[3][3] += a[3] * b[3];
                        }
                    }
                    (Packed { ptr: a }, OffsetsAndPtrs { row_byte_offsets, col_ptrs }) => {
                        let pb0 = *(col_ptrs.offset(0));
                        let pb1 = *(col_ptrs.offset(1));
                        let pb2 = *(col_ptrs.offset(2));
                        let pb3 = *(col_ptrs.offset(3));
                        for i in 0..k {
                            let a = std::slice::from_raw_parts(a.offset(4 * i as isize), 4);
                            let offset = *row_byte_offsets.offset(i as isize)
                                / std::mem::size_of::<TB>() as isize;
                            let b0 = *(pb0.offset(offset));
                            let b1 = *(pb1.offset(offset));
                            let b2 = *(pb2.offset(offset));
                            let b3 = *(pb3.offset(offset));
                            ab[0][0] += a[0].as_() * b0.as_();
                            ab[0][1] += a[0].as_() * b1.as_();
                            ab[0][2] += a[0].as_() * b2.as_();
                            ab[0][3] += a[0].as_() * b3.as_();
                            ab[1][0] += a[1].as_() * b0.as_();
                            ab[1][1] += a[1].as_() * b1.as_();
                            ab[1][2] += a[1].as_() * b2.as_();
                            ab[1][3] += a[1].as_() * b3.as_();
                            ab[2][0] += a[2].as_() * b0.as_();
                            ab[2][1] += a[2].as_() * b1.as_();
                            ab[2][2] += a[2].as_() * b2.as_();
                            ab[2][3] += a[2].as_() * b3.as_();
                            ab[3][0] += a[3].as_() * b0.as_();
                            ab[3][1] += a[3].as_() * b1.as_();
                            ab[3][2] += a[3].as_() * b2.as_();
                            ab[3][3] += a[3].as_() * b3.as_();
                        }
                    }
                    (Packed { ptr: a }, VecStride { ptr: b, byte_stride }) => {
                        for i in 0..k {
                            let a = std::slice::from_raw_parts(a.offset(4 * i as isize), 4);
                            let b = *b.offset(
                                i as isize * byte_stride / std::mem::size_of::<TB>() as isize,
                            );
                            ab[0][0] += a[0].as_() * b.as_();
                            ab[1][0] += a[1].as_() * b.as_();
                            ab[2][0] += a[2].as_() * b.as_();
                            ab[3][0] += a[3].as_() * b.as_();
                        }
                    }
                    _ => return 1,
                }
            }
            let mut pnl = spec.non_linear;
            loop {
                if pnl.is_null() {
                    break;
                }
                match *pnl {
                    FusedKerSpec::Done => break,
                    FusedKerSpec::AddC => match *spec.c {
                        Strides { ptr: c, row_byte_stride, col_byte_stride } => {
                            let rsc = row_byte_stride as usize / std::mem::size_of::<TC>();
                            let csc = col_byte_stride as usize / std::mem::size_of::<TC>();
                            let c = std::slice::from_raw_parts_mut(c, 1 + 3 * csc + 3 * rsc);
                            ab[0][0] += c[0 * csc + 0 * rsc].as_();
                            ab[0][1] += c[1 * csc + 0 * rsc].as_();
                            ab[0][2] += c[2 * csc + 0 * rsc].as_();
                            ab[0][3] += c[3 * csc + 0 * rsc].as_();
                            ab[1][0] += c[0 * csc + 1 * rsc].as_();
                            ab[1][1] += c[1 * csc + 1 * rsc].as_();
                            ab[1][2] += c[2 * csc + 1 * rsc].as_();
                            ab[1][3] += c[3 * csc + 1 * rsc].as_();
                            ab[2][0] += c[0 * csc + 2 * rsc].as_();
                            ab[2][1] += c[1 * csc + 2 * rsc].as_();
                            ab[2][2] += c[2 * csc + 2 * rsc].as_();
                            ab[2][3] += c[3 * csc + 2 * rsc].as_();
                            ab[3][0] += c[0 * csc + 3 * rsc].as_();
                            ab[3][1] += c[1 * csc + 3 * rsc].as_();
                            ab[3][2] += c[2 * csc + 3 * rsc].as_();
                            ab[3][3] += c[3 * csc + 3 * rsc].as_();
                        }
                        _ => return 1,
                    },
                    FusedKerSpec::PerRowMul(bias) => {
                        for i in 0..4 {
                            ab[i][0] *= *bias.offset(i as isize);
                            ab[i][1] *= *bias.offset(i as isize);
                            ab[i][2] *= *bias.offset(i as isize);
                            ab[i][3] *= *bias.offset(i as isize);
                        }
                    }
                    FusedKerSpec::PerRowAdd(bias) => {
                        for i in 0..4 {
                            ab[i][0] += *bias.offset(i as isize);
                            ab[i][1] += *bias.offset(i as isize);
                            ab[i][2] += *bias.offset(i as isize);
                            ab[i][3] += *bias.offset(i as isize);
                        }
                    }
                    FusedKerSpec::PerColMul(bias) => {
                        for i in 0..4 {
                            ab[0][i] *= *bias.offset(i as isize);
                            ab[1][i] *= *bias.offset(i as isize);
                            ab[2][i] *= *bias.offset(i as isize);
                            ab[3][i] *= *bias.offset(i as isize);
                        }
                    }
                    FusedKerSpec::PerColAdd(bias) => {
                        for i in 0..4 {
                            ab[0][i] += *bias.offset(i as isize);
                            ab[1][i] += *bias.offset(i as isize);
                            ab[2][i] += *bias.offset(i as isize);
                            ab[3][i] += *bias.offset(i as isize);
                        }
                    }
                    FusedKerSpec::Min(m) => {
                        for i in 0..4 {
                            for j in 0..4 {
                                ab[i][j] = if m < ab[i][j] { m } else { ab[i][j] }
                            }
                        }
                    }
                    FusedKerSpec::Max(m) => {
                        for i in 0..4 {
                            for j in 0..4 {
                                ab[i][j] = if m > ab[i][j] { m } else { ab[i][j] }
                            }
                        }
                    }
                }
                pnl = pnl.add(1);
            }
            match *spec.c {
                Strides { ptr: c, row_byte_stride, col_byte_stride } => {
                    let rsc = row_byte_stride as usize / std::mem::size_of::<TC>();
                    let csc = col_byte_stride as usize / std::mem::size_of::<TC>();
                    let c = std::slice::from_raw_parts_mut(c, 1 + 3 * csc + 3 * rsc);
                    c[0 * csc + 0 * rsc] = ab[0][0].as_();
                    c[1 * csc + 0 * rsc] = ab[0][1].as_();
                    c[2 * csc + 0 * rsc] = ab[0][2].as_();
                    c[3 * csc + 0 * rsc] = ab[0][3].as_();
                    c[0 * csc + 1 * rsc] = ab[1][0].as_();
                    c[1 * csc + 1 * rsc] = ab[1][1].as_();
                    c[2 * csc + 1 * rsc] = ab[1][2].as_();
                    c[3 * csc + 1 * rsc] = ab[1][3].as_();
                    c[0 * csc + 2 * rsc] = ab[2][0].as_();
                    c[1 * csc + 2 * rsc] = ab[2][1].as_();
                    c[2 * csc + 2 * rsc] = ab[2][2].as_();
                    c[3 * csc + 2 * rsc] = ab[2][3].as_();
                    c[0 * csc + 3 * rsc] = ab[3][0].as_();
                    c[1 * csc + 3 * rsc] = ab[3][1].as_();
                    c[2 * csc + 3 * rsc] = ab[3][2].as_();
                    c[3 * csc + 3 * rsc] = ab[3][3].as_();
                }
                VecStride { ptr: c, byte_stride } => {
                    let stride = byte_stride / std::mem::size_of::<TC>() as isize;
                    let c: *mut TC = c as _;
                    *c.offset(0 * stride) = ab[0][0].as_();
                    *c.offset(1 * stride) = ab[1][0].as_();
                    *c.offset(2 * stride) = ab[2][0].as_();
                    *c.offset(3 * stride) = ab[3][0].as_();
                }
                _ => return 1,
            }
        }
        return 0;
    }
}

#[cfg(test)]
#[derive(Copy, Clone, Debug)]
pub struct GenericMmmTest3x2<TA, TB, TC, TI>(PhantomData<(TA, TB, TC, TI)>);

#[cfg(test)]
unsafe impl<TA, TB, TC, TI> Send for GenericMmmTest3x2<TA, TB, TC, TI> {}

#[cfg(test)]
unsafe impl<TA, TB, TC, TI> Sync for GenericMmmTest3x2<TA, TB, TC, TI> {}

#[cfg(test)]
impl<TA, TB, TC, TI> MatMatMulKer<TA, TB, TC, TI> for GenericMmmTest3x2<TA, TB, TC, TI>
where
    TA: Copy + fmt::Debug + AsPrimitive<TI>,
    TB: Copy + fmt::Debug + AsPrimitive<TI>,
    TC: Copy + fmt::Debug + AsPrimitive<TI> + 'static,
    TI: Copy
        + ops::AddAssign
        + ops::SubAssign
        + ops::Mul<Output = TI>
        + ops::MulAssign
        + PartialOrd
        + Zero
        + fmt::Debug
        + fmt::Display
        + AsPrimitive<TC>
        + 'static,
{
    #[inline(always)]
    fn name() -> &'static str {
        "generic-test-3x2"
    }
    #[inline(always)]
    fn mr() -> usize {
        3
    }
    #[inline(always)]
    fn nr() -> usize {
        2
    }
    #[inline(always)]
    fn alignment_bytes_packed_a() -> usize {
        std::mem::size_of::<TA>()
    }
    #[inline(always)]
    fn alignment_bytes_packed_b() -> usize {
        std::mem::size_of::<TB>()
    }
    #[inline(never)]
    fn kernel(spec: &MatMatMulKerSpec<TA, TB, TC, TI>) -> isize {
        unsafe {
            let mut ab = [[TI::zero(); 2]; 3];
            if let Mul { k, zero_point_a, zero_point_b } = *spec.linear {
                match (*spec.a, *spec.b) {
                    (Packed { ptr: pa }, Packed { ptr: pb }) => {
                        for i in 0..k {
                            let mut a = [TI::zero(); 3];
                            let mut b = [TI::zero(); 2];
                            for ix in 0..3 {
                                a[ix] = (*pa.offset(((3 * i) + ix) as isize)).as_();
                                if !zero_point_a.is_null() {
                                    a[ix] -= *zero_point_a.offset(ix as isize);
                                }
                            }
                            for ix in 0..2 {
                                b[ix] = (*pb.offset(((2 * i) + ix) as isize)).as_();
                                if !zero_point_b.is_null() {
                                    b[ix] -= *zero_point_b.offset(ix as isize);
                                }
                            }
                            ab[0][0] += a[0] * b[0];
                            ab[0][1] += a[0] * b[1];
                            ab[1][0] += a[1] * b[0];
                            ab[1][1] += a[1] * b[1];
                            ab[2][0] += a[2] * b[0];
                            ab[2][1] += a[2] * b[1];
                        }
                    }
                    (Packed { ptr: a }, OffsetsAndPtrs { row_byte_offsets, col_ptrs }) => {
                        let pb0 = *(col_ptrs.offset(0));
                        let pb1 = *(col_ptrs.offset(1));
                        for i in 0..k {
                            let a = std::slice::from_raw_parts(a.offset(3 * i as isize), 3);
                            let offset =
                                *row_byte_offsets.offset(i as isize) / std::mem::size_of::<TB>() as isize;
                            let b0 = *(pb0.offset(offset));
                            let b1 = *(pb1.offset(offset));
                            ab[0][0] += a[0].as_() * b0.as_();
                            ab[0][1] += a[0].as_() * b1.as_();
                            ab[1][0] += a[1].as_() * b0.as_();
                            ab[1][1] += a[1].as_() * b1.as_();
                            ab[2][0] += a[2].as_() * b0.as_();
                            ab[2][1] += a[2].as_() * b1.as_();
                        }
                    }
                    (Packed { ptr: a }, VecStride { ptr: b, byte_stride }) => {
                        for i in 0..k {
                            let a = std::slice::from_raw_parts(a.offset(3 * i as isize), 3);
                            let b = *b.offset(i as isize * byte_stride / std::mem::size_of::<TB>() as isize);
                            ab[0][0] += a[0].as_() * b.as_();
                            ab[1][0] += a[1].as_() * b.as_();
                            ab[2][0] += a[2].as_() * b.as_();
                        }
                    }
                    _ => return 1,
                }
                let mut pnl = spec.non_linear;
                loop {
                    if pnl.is_null() {
                        break;
                    }
                    match *pnl {
                        FusedKerSpec::Done => break,
                        FusedKerSpec::AddC => match *spec.c {
                            Strides { ptr: c, row_byte_stride, col_byte_stride } => {
                                let rsc = row_byte_stride as usize / std::mem::size_of::<TC>();
                                let csc = col_byte_stride as usize / std::mem::size_of::<TC>();
                                let c = std::slice::from_raw_parts_mut(c, 1 + 1 * csc + 2 * rsc);
                                ab[0][0] += c[0 * csc + 0 * rsc].as_();
                                ab[0][1] += c[1 * csc + 0 * rsc].as_();
                                ab[1][0] += c[0 * csc + 1 * rsc].as_();
                                ab[1][1] += c[1 * csc + 1 * rsc].as_();
                                ab[2][0] += c[0 * csc + 2 * rsc].as_();
                                ab[2][1] += c[1 * csc + 2 * rsc].as_();
                            }
                            _ => return 1,
                        },
                        FusedKerSpec::PerRowMul(bias) => {
                            for i in 0..3 {
                                ab[i][0] *= *bias.offset(i as isize);
                                ab[i][1] *= *bias.offset(i as isize);
                            }
                        }
                        FusedKerSpec::PerRowAdd(bias) => {
                            for i in 0..3 {
                                ab[i][0] += *bias.offset(i as isize);
                                ab[i][1] += *bias.offset(i as isize);
                            }
                        }
                        FusedKerSpec::PerColMul(bias) => {
                            for i in 0..2 {
                                ab[0][i] *= *bias.offset(i as isize);
                                ab[1][i] *= *bias.offset(i as isize);
                                ab[2][i] *= *bias.offset(i as isize);
                            }
                        }
                        FusedKerSpec::PerColAdd(bias) => {
                            for i in 0..2 {
                                ab[0][i] += *bias.offset(i as isize);
                                ab[1][i] += *bias.offset(i as isize);
                                ab[2][i] += *bias.offset(i as isize);
                            }
                        }
                        FusedKerSpec::Min(m) => {
                            for i in 0..3 {
                                for j in 0..2 {
                                    ab[i][j] = if m < ab[i][j] { m } else { ab[i][j] }
                                }
                            }
                        }
                        FusedKerSpec::Max(m) => {
                            for i in 0..3 {
                                for j in 0..2 {
                                    ab[i][j] = if m > ab[i][j] { m } else { ab[i][j] }
                                }
                            }
                        }
                    }
                    pnl = pnl.add(1);
                }
                match *spec.c {
                    Strides { ptr: c, row_byte_stride, col_byte_stride } => {
                        let rsc = row_byte_stride as usize / std::mem::size_of::<TC>();
                        let csc = col_byte_stride as usize / std::mem::size_of::<TC>();
                        let c = std::slice::from_raw_parts_mut(c, 1 + 3 * csc + 3 * rsc);
                        c[0 * csc + 0 * rsc] = ab[0][0].as_();
                        c[1 * csc + 0 * rsc] = ab[0][1].as_();
                        c[0 * csc + 1 * rsc] = ab[1][0].as_();
                        c[1 * csc + 1 * rsc] = ab[1][1].as_();
                        c[0 * csc + 2 * rsc] = ab[2][0].as_();
                        c[1 * csc + 2 * rsc] = ab[2][1].as_();
                    }
                    VecStride { ptr: c, byte_stride } => {
                        let stride = byte_stride / std::mem::size_of::<TC>() as isize;
                        let c: *mut TC = c as _;
                        *c.offset(0 * stride) = ab[0][0].as_();
                        *c.offset(1 * stride) = ab[1][0].as_();
                        *c.offset(2 * stride) = ab[2][0].as_();
                    }
                    _ => return 1,
                }
            }
        }
        return 0;
    }
}

#[cfg(test)]
mod test_3_2 {
    mmm_kernel_tests!(true, crate::generic::mmm::GenericMmmTest3x2<f32, f32, f32, f32>, f32, f32, f32, f32);
    mmm_frame_tests!(true, crate::generic::mmm::GenericMmmTest3x2<f32, f32, f32, f32>, f32, f32, f32, f32);
}

#[cfg(test)]
mod test {
    mmm_kernel_tests!(true, crate::generic::GenericMmm4x4<f32, f32, f32, f32>, f32, f32, f32, f32);
    mmm_frame_tests!(true, crate::generic::GenericMmm4x4<f32, f32, f32, f32>, f32, f32, f32, f32);
}
