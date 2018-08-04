use ndarray::prelude::*;
use std::collections::HashMap;
use Result;

use ops::Attr;

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub enum DataFormat {
    NHWC,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serialize", derive(Serialize))]
pub enum Padding {
    Valid,
    Same,
}

pub struct ImageWrapper<'a, T: 'a>(ArrayView3<'a, T>);

impl<'a, T> ImageWrapper<'a, T> {
    pub fn height(&self) -> usize {
        self.0.shape()[0]
    }
    pub fn width(&self) -> usize {
        self.0.shape()[1]
    }
    pub fn depth(&self) -> usize {
        self.0.shape()[2]
    }
}

pub struct BatchImageWrapper<'a, T: 'a>(pub ArrayView4<'a, T>);

#[allow(dead_code)]
impl<'a, T> BatchImageWrapper<'a, T> {
    pub fn count(&self) -> usize {
        self.0.shape()[0]
    }
    pub fn n(&self) -> usize {
        self.0.shape()[0]
    }
    pub fn height(&self) -> usize {
        self.0.shape()[1]
    }
    pub fn h(&self) -> usize {
        self.0.shape()[1]
    }
    pub fn width(&self) -> usize {
        self.0.shape()[2]
    }
    pub fn w(&self) -> usize {
        self.0.shape()[2]
    }
    pub fn depth(&self) -> usize {
        self.0.shape()[3]
    }
    pub fn d(&self) -> usize {
        self.0.shape()[3]
    }
}

#[derive(Debug, Clone, new)]
pub struct LocalPatch {
    pub _data_format: DataFormat,
    pub padding: Padding,
    pub h_stride: usize,
    pub v_stride: usize,
}

impl LocalPatch {
    pub fn same(v_stride: usize, h_stride: usize) -> LocalPatch {
        LocalPatch {
            _data_format: DataFormat::NHWC,
            h_stride,
            v_stride,
            padding: Padding::Same,
        }
    }

    pub fn valid(v_stride: usize, h_stride: usize) -> LocalPatch {
        LocalPatch {
            _data_format: DataFormat::NHWC,
            h_stride,
            v_stride,
            padding: Padding::Valid,
        }
    }

    pub fn build(pb: &::tfpb::node_def::NodeDef) -> Result<LocalPatch> {
        let data_format = pb.get_attr_opt_raw_str("data_format")?.unwrap_or(b"NHWC");
        if data_format == b"NCHW" {
            Err("NCHW data_format not implemented")?
        }
        let strides: Vec<usize> = pb.get_attr_list_int("strides")?;
        if strides.len() != 4 || strides[0] != 1 && strides[3] != 1 {
            Err(format!(
                "strides must be of the form [1, h, v, 1], found {:?}",
                strides
            ))?
        };
        let v_stride = strides[1];
        let h_stride = strides[2];
        let padding = pb.get_attr_raw_str("padding")?;
        let padding = match padding {
            b"VALID" => Padding::Valid,
            b"SAME" => Padding::Same,
            s => Err(format!(
                "unsupported Padding {}",
                String::from_utf8_lossy(s)
            ))?,
        };
        Ok(LocalPatch {
            _data_format: DataFormat::NHWC,
            padding,
            h_stride,
            v_stride,
        })
    }

    /// Appends each attribute of the LocalPatch to the given hashmap.
    /// Returns the attributes of the LocalPatch and their values.
    pub fn get_attributes(&self) -> HashMap<&'static str, Attr> {
        hashmap! {
            "data_format" => Attr::DataFormat(self._data_format),
            "padding" => Attr::Padding(self.padding),
            "strides" => Attr::UsizeVec(vec![1, self.v_stride, self.h_stride, 1]),
        }
    }

    pub fn adjusted_dim(
        &self,
        in_rows: usize,
        in_cols: usize,
        (filter_rows, filter_cols): (usize, usize),
    ) -> (usize, usize) {
        (
            self.adjusted_dim_rows(in_rows, filter_rows),
            self.adjusted_dim_cols(in_cols, filter_cols),
        )
    }

    pub fn adjusted_dim_rows(&self, in_rows: usize, filter_rows: usize) -> usize {
        match self.padding {
            Padding::Same => (in_rows as f32 / self.v_stride as f32).ceil() as usize,
            Padding::Valid => {
                ((in_rows - filter_rows + 1) as f32 / self.v_stride as f32).ceil() as usize
            }
        }
    }

    pub fn adjusted_dim_cols(&self, in_cols: usize, filter_cols: usize) -> usize {
        match self.padding {
            Padding::Same => (in_cols as f32 / self.h_stride as f32).ceil() as usize,
            Padding::Valid => {
                ((in_cols - filter_cols + 1) as f32 / self.h_stride as f32).ceil() as usize
            }
        }
    }

    pub fn pad<T>(
        &self,
        data: ArrayView4<T>,
        shape: (usize, usize),
        item: T,
        pad_rows: bool,
        pad_cols: bool,
    ) -> Result<Option<Array4<T>>>
    where
        T: Copy + ::num_traits::Zero + ::std::fmt::Debug,
    {
        // The pad_rows and pad_cols arguments are used for streaming evaluation,
        // where we don't want to pad along the streaming dimension, even if the
        // padding is set to VALID.

        let img = BatchImageWrapper(data);
        let (filter_rows, filter_cols) = shape;

        if self.padding == Padding::Same {
            // https://www.tensorflow.org/api_guides/python/nn#Convolution
            let padded_cols = if pad_cols {
                let h_padding = ::std::cmp::max(
                    0,
                    filter_cols - if img.width() % self.h_stride == 0 {
                        self.h_stride
                    } else {
                        img.width() % self.h_stride
                    },
                );
                let left_padding = h_padding / 2;
                let right_padding = h_padding - left_padding;
                let left_padding = ::ndarray::Array4::<T>::from_elem(
                    (img.count(), img.height(), left_padding, img.depth()),
                    item,
                );
                let right_padding = ::ndarray::Array4::<T>::from_elem(
                    (img.count(), img.height(), right_padding, img.depth()),
                    item,
                );

                ::ndarray::stack(
                    ::ndarray::Axis(2),
                    &[left_padding.view(), data.view(), right_padding.view()],
                )?
            } else {
                data.to_owned()
            };

            let padded_rows = if pad_rows {
                let v_padding = ::std::cmp::max(
                    0,
                    filter_rows - if img.height() % self.v_stride == 0 {
                        self.v_stride
                    } else {
                        img.height() % self.v_stride
                    },
                );
                let top_padding = v_padding / 2;
                let bottom_padding = v_padding - top_padding;
                let top_padding = ::ndarray::Array4::<T>::from_elem(
                    (
                        img.count(),
                        top_padding,
                        padded_cols.shape()[2],
                        img.depth(),
                    ),
                    item,
                );
                let bottom_padding = ::ndarray::Array4::<T>::from_elem(
                    (
                        img.count(),
                        bottom_padding,
                        padded_cols.shape()[2],
                        img.depth(),
                    ),
                    item,
                );

                ::ndarray::stack(
                    ::ndarray::Axis(1),
                    &[
                        top_padding.view(),
                        padded_cols.view(),
                        bottom_padding.view(),
                    ],
                )?
            } else {
                padded_cols
            };

            Ok(Some(padded_rows))
        } else {
            Ok(None)
        }
    }

    // data is expected in HWC
    pub fn mk_patches<T: Copy + ::num_traits::Zero + ::std::fmt::Debug>(
        &self,
        data: ArrayView<T, Ix3>,
        shape: (usize, usize),
        pad_rows: bool,
        pad_cols: bool,
    ) -> Result<Array2<T>> {
        let img = ImageWrapper(data);
        let (filter_rows, filter_cols) = shape;

        let (out_height, out_width) =
            self.adjusted_dim(img.height(), img.width(), (filter_rows, filter_cols));

        let patches_size = (
            (out_height * out_width) as usize,
            filter_rows * filter_cols * img.depth(),
        );

        let mut patches = unsafe { ::ndarray::Array2::<T>::uninitialized(patches_size) };
        let data = data.into_shape((1, img.height(), img.width(), img.depth()))?;
        let padded = self.pad(
            data,
            (filter_rows, filter_cols),
            T::zero(),
            pad_rows,
            pad_cols,
        )?;
        let data = padded.as_ref().map(|a| a.view()).unwrap_or(data.view());
        for i_x in 0..out_width {
            for i_y in 0..out_height {
                let mut patch_row = patches.row_mut(i_y * out_width + i_x);
                for f_x in 0..filter_cols {
                    for f_y in 0..filter_rows {
                        for d in 0..img.depth() {
                            let loc = &mut patch_row
                                [f_y * img.depth() * filter_cols + f_x * img.depth() + d];
                            *loc =
                                data[(0, i_y * self.v_stride + f_y, i_x * self.h_stride + f_x, d)];
                        }
                    }
                }
            }
        }
        Ok(patches)
    }
}

pub fn into_4d<T>(data: ArrayD<T>) -> Result<Array4<T>> {
    if data.shape().len() != 4 {
        Err(format!("Expected 4D shape, found: {:?}", data.shape()))?
    }
    let shape = (
        data.shape()[0],
        data.shape()[1],
        data.shape()[2],
        data.shape()[3],
    );
    Ok(data.into_shape(shape)?)
}
