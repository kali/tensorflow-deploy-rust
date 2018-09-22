pub fn multi_broadcast(shapes: &[impl AsRef<[usize]>]) -> Option<Vec<usize>> {
    let len = shapes.iter().map(|shape| shape.as_ref().len()).max()?;
    let mut shape = Vec::with_capacity(len);
    for i in 0..len {
        let mut wanted_size = 1;
        for shape in shapes {
            let len = shape.as_ref().len();
            let dim = if i < len {
                shape.as_ref()[len - i - 1]
            } else {
                1
            };
            if dim != 1 {
                if wanted_size != 1 && dim != wanted_size {
                    return None;
                }
                wanted_size = dim;
            }
        }
        shape.push(wanted_size)
    }
    shape.reverse();
    return Some(shape);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn onnx_1() {
        assert_eq!(
            multi_broadcast(&vec![vec![2, 3, 4, 5], vec![]]),
            Some(vec![2, 3, 4, 5])
        )
    }

    #[test]
    fn onnx_2() {
        assert_eq!(
            multi_broadcast(&vec![vec![2, 3, 4, 5], vec![5]]),
            Some(vec![2, 3, 4, 5])
        )
    }

    #[test]
    fn onnx_3() {
        assert_eq!(
            multi_broadcast(&vec![vec![4, 5], vec![2, 3, 4, 5]]),
            Some(vec![2, 3, 4, 5])
        )
    }

    #[test]
    fn onnx_4() {
        assert_eq!(
            multi_broadcast(&vec![vec![1, 4, 5], vec![2, 3, 4, 1]]),
            Some(vec![2, 3, 4, 5])
        )
    }

    #[test]
    fn onnx_5() {
        assert_eq!(
            multi_broadcast(&vec![vec![3, 4, 5], vec![2, 1, 1, 1]]),
            Some(vec![2, 3, 4, 5])
        )
    }
}
