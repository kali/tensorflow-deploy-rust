use std::fmt::Debug;
use std::marker::PhantomData;
use tract_linalg::MatMul;

#[derive(Copy, Clone, Debug, new)]
pub struct NdArrayDummyPackedMatMul<T: ndarray::LinalgScalar + Copy> {
    m: usize,
    k: usize,
    n: usize,
    _junk: PhantomData<T>,
}

impl<T: ndarray::LinalgScalar + Copy + Send + Sync + Debug> MatMul<T>
    for NdArrayDummyPackedMatMul<T>
{
    fn packed_a_len(&self) -> usize {
        self.m * self.k
    }
    fn pack_a(&self, pa: *mut T, a: *const T, rsa: isize, csa: isize) {
        unsafe {
            for x in 0..self.k as isize {
                for y in 0..self.m as isize {
                    *pa.offset(x + y * self.k as isize) = *a.offset(x * csa + y * rsa)
                }
            }
        }
    }
    fn packed_b_len(&self) -> usize {
        self.k * self.n
    }
    fn pack_b(&self, pb: *mut T, b: *const T, rsb: isize, csb: isize) {
        unsafe {
            for x in 0..self.n as isize {
                for y in 0..self.k as isize {
                    *pb.offset(x + y * self.n as isize) = *b.offset(x * csb + y * rsb)
                }
            }
        }
    }

    fn mat_mul_prepacked(&self, pa: *const T, pb: *const T, pc: *mut T, rsc: isize, csc: isize) {
        unsafe {
            assert_eq!(rsc, self.n as isize);
            assert_eq!(csc, 1);
            let a = ndarray::ArrayView::from_shape_ptr((self.m, self.k), pa);
            let b = ndarray::ArrayView::from_shape_ptr((self.k, self.n), pb);
            let mut c = ndarray::ArrayViewMut::from_shape_ptr((self.m, self.n), pc);
            ndarray::linalg::general_mat_mul(T::one(), &a, &b, T::zero(), &mut c);
        }
    }
}
