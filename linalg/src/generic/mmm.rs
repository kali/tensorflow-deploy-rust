use num_traits::{AsPrimitive, Zero};
use std::marker::PhantomData;
use std::{fmt, ops};

use crate::frame::mmm::LinearSpec::*;
use crate::frame::mmm::PanelStore::*;
use crate::frame::mmm::*;

#[derive(Copy, Clone, Debug)]
pub struct GenericMmm4x4<TA, TB, TC, TI>(PhantomData<(TA, TB, TC, TI)>)
where
    TA: Copy + AsPrimitive<TI>,
    TB: Copy + AsPrimitive<TI>,
    TC: Copy + AsPrimitive<TI> + 'static,
    TI: Copy
        + ops::Add
        + ops::AddAssign
        + ops::Mul<Output = TI>
        + ops::MulAssign
        + PartialOrd
        + Zero
        + AsPrimitive<TC>
        + 'static;

impl<TA, TB, TC, TI> MatMatMulKer<TA, TB, TC, TI> for GenericMmm4x4<TA, TB, TC, TI>
where
    TA: Copy
        + ops::Add
        + ops::Mul
        + Zero
        + fmt::Debug
        + fmt::Display
        + PartialEq
        + Send
        + Sync
        + AsPrimitive<TI>,
    TB: Copy
        + ops::Add
        + ops::Mul
        + Zero
        + fmt::Debug
        + fmt::Display
        + PartialEq
        + Send
        + Sync
        + AsPrimitive<TI>,
    TC: Copy
        + ops::Add
        + ops::Mul
        + Zero
        + fmt::Debug
        + fmt::Display
        + PartialEq
        + Send
        + Sync
        + AsPrimitive<TI>
        + 'static,
    TI: Copy
        + ops::Add
        + ops::AddAssign
        + ops::Mul<Output = TI>
        + ops::MulAssign
        + PartialOrd
        + Zero
        + fmt::Debug
        + fmt::Display
        + PartialEq
        + Send
        + Sync
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
            match (*spec.a, *spec.b, *spec.linear) {
                (Packed { ptr: a }, Packed { ptr: b }, Mul { k }) => {
                    for i in 0..k {
                        let a = std::slice::from_raw_parts(a.offset(4 * i as isize), 4);
                        let b = std::slice::from_raw_parts(b.offset(4 * i as isize), 4);
                        ab[0][0] += a[0].as_() * b[0].as_();
                        ab[0][1] += a[0].as_() * b[1].as_();
                        ab[0][2] += a[0].as_() * b[2].as_();
                        ab[0][3] += a[0].as_() * b[3].as_();
                        ab[1][0] += a[1].as_() * b[0].as_();
                        ab[1][1] += a[1].as_() * b[1].as_();
                        ab[1][2] += a[1].as_() * b[2].as_();
                        ab[1][3] += a[1].as_() * b[3].as_();
                        ab[2][0] += a[2].as_() * b[0].as_();
                        ab[2][1] += a[2].as_() * b[1].as_();
                        ab[2][2] += a[2].as_() * b[2].as_();
                        ab[2][3] += a[2].as_() * b[3].as_();
                        ab[3][0] += a[3].as_() * b[0].as_();
                        ab[3][1] += a[3].as_() * b[1].as_();
                        ab[3][2] += a[3].as_() * b[2].as_();
                        ab[3][3] += a[3].as_() * b[3].as_();
                    }
                }
                (Packed { ptr: a }, OffsetsAndPtrs { row_byte_offsets, col_ptrs }, Mul { k }) => {
                    let pb0 = *(col_ptrs.offset(0));
                    let pb1 = *(col_ptrs.offset(1));
                    let pb2 = *(col_ptrs.offset(2));
                    let pb3 = *(col_ptrs.offset(3));
                    for i in 0..k {
                        let a = std::slice::from_raw_parts(a.offset(4 * i as isize), 4);
                        let offset = *row_byte_offsets.offset(i as isize) / 4;
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
                (Packed { ptr: a }, VecStride { ptr: b, byte_stride }, Mul { k }) => {
                    for i in 0..k {
                        let a = std::slice::from_raw_parts(a.offset(4 * i as isize), 4);
                        let b = *b.offset(i as isize * byte_stride / 4);
                        ab[0][0] += a[0].as_() * b.as_();
                        ab[1][0] += a[1].as_() * b.as_();
                        ab[2][0] += a[2].as_() * b.as_();
                        ab[3][0] += a[3].as_() * b.as_();
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
                            let rsc = row_byte_stride as usize / 4;
                            let csc = col_byte_stride as usize / 4;
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
                    let rsc = row_byte_stride as usize / 4;
                    let csc = col_byte_stride as usize / 4;
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
                    let stride = byte_stride / 4;
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
pub struct GenericMmmTest3x2<TA, TB, TC, TI>(PhantomData<(TA, TB, TC, TI)>)
where
    TA: Copy
        + ops::Add
        + ops::Mul
        + Zero
        + fmt::Debug
        + fmt::Display
        + PartialEq
        + Send
        + Sync
        + AsPrimitive<TI>,
    TB: Copy
        + ops::Add
        + ops::Mul
        + Zero
        + fmt::Debug
        + fmt::Display
        + PartialEq
        + Send
        + Sync
        + AsPrimitive<TI>,
    TC: Copy
        + ops::Add
        + ops::Mul
        + Zero
        + fmt::Debug
        + fmt::Display
        + PartialEq
        + Send
        + Sync
        + AsPrimitive<TI>
        + 'static,
    TI: Copy
        + ops::Add
        + ops::AddAssign
        + ops::Mul<Output = TI>
        + ops::MulAssign
        + PartialOrd
        + Zero
        + fmt::Debug
        + fmt::Display
        + PartialEq
        + Send
        + Sync
        + AsPrimitive<TC>
        + 'static;

#[cfg(test)]
impl<TA, TB, TC, TI> MatMatMulKer<TA, TB, TC, TI> for GenericMmmTest3x2<TA, TB, TC, TI>
where
    TA: Copy
        + ops::Add
        + ops::Mul
        + Zero
        + fmt::Debug
        + fmt::Display
        + PartialEq
        + Send
        + Sync
        + AsPrimitive<TI>,
    TB: Copy
        + ops::Add
        + ops::Mul
        + Zero
        + fmt::Debug
        + fmt::Display
        + PartialEq
        + Send
        + Sync
        + AsPrimitive<TI>,
    TC: Copy
        + ops::Add
        + ops::Mul
        + Zero
        + fmt::Debug
        + fmt::Display
        + PartialEq
        + Send
        + Sync
        + AsPrimitive<TI>
        + 'static,
    TI: Copy
        + ops::Add
        + ops::AddAssign
        + ops::Mul<Output = TI>
        + ops::MulAssign
        + PartialOrd
        + Zero
        + fmt::Debug
        + fmt::Display
        + PartialEq
        + Send
        + Sync
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
            match (*spec.a, *spec.b, *spec.linear) {
                (Packed { ptr: a }, Packed { ptr: b }, Mul { k }) => {
                    for i in 0..k {
                        let a = std::slice::from_raw_parts(a.offset(3 * i as isize), 3);
                        let b = std::slice::from_raw_parts(b.offset(2 * i as isize), 2);
                        ab[0][0] += a[0].as_() * b[0].as_();
                        ab[0][1] += a[0].as_() * b[1].as_();
                        ab[1][0] += a[1].as_() * b[0].as_();
                        ab[1][1] += a[1].as_() * b[1].as_();
                        ab[2][0] += a[2].as_() * b[0].as_();
                        ab[2][1] += a[2].as_() * b[1].as_();
                    }
                }
                (Packed { ptr: a }, OffsetsAndPtrs { row_byte_offsets, col_ptrs }, Mul { k }) => {
                    let pb0 = *(col_ptrs.offset(0));
                    let pb1 = *(col_ptrs.offset(1));
                    for i in 0..k {
                        let a = std::slice::from_raw_parts(a.offset(3 * i as isize), 3);
                        let offset = *row_byte_offsets.offset(i as isize) / 4;
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
                (Packed { ptr: a }, VecStride { ptr: b, byte_stride }, Mul { k }) => {
                    for i in 0..k {
                        let a = std::slice::from_raw_parts(a.offset(3 * i as isize), 3);
                        let b = *b.offset(i as isize * byte_stride / 4);
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
                            let rsc = row_byte_stride as usize / 4;
                            let csc = col_byte_stride as usize / 4;
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
                    let rsc = row_byte_stride as usize / 4;
                    let csc = col_byte_stride as usize / 4;
                    let c = std::slice::from_raw_parts_mut(c, 1 + 3 * csc + 3 * rsc);
                    c[0 * csc + 0 * rsc] = ab[0][0].as_();
                    c[1 * csc + 0 * rsc] = ab[0][1].as_();
                    c[0 * csc + 1 * rsc] = ab[1][0].as_();
                    c[1 * csc + 1 * rsc] = ab[1][1].as_();
                    c[0 * csc + 2 * rsc] = ab[2][0].as_();
                    c[1 * csc + 2 * rsc] = ab[2][1].as_();
                }
                VecStride { ptr: c, byte_stride } => {
                    let stride = byte_stride / 4;
                    let c: *mut TC = c as _;
                    *c.offset(0 * stride) = ab[0][0].as_();
                    *c.offset(1 * stride) = ab[1][0].as_();
                    *c.offset(2 * stride) = ab[2][0].as_();
                }
                _ => return 1,
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
