use tract_onnx::prelude::*;

fn main() -> TractResult<()> {
    let mut model = tract_onnx::onnx().model_for_path("resnet.onnx")?;

    model.set_input_fact(0, InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 3, 224, 224)))?;

    // optimize the model and get an execution plan
    let model = model.into_optimized()?;
    let plan = SimplePlan::new(model)?;

    let img = image::open("elephants.jpg").unwrap().to_rgb();
    let resized = image::imageops::resize(&img, 224, 224, ::image::imageops::FilterType::Triangle);
    let image: Tensor = tract_ndarray::Array4::from_shape_fn((1, 3, 224, 224), |(_, c, y, x)| {
        resized[(x as _, y as _)][c] as f32 / 255.0
    })
    .into();

    let result = plan.run(tvec!(image))?;

    // find and display the max value with its index
    let best = result[0]
        .to_array_view::<f32>()?
        .iter()
        .cloned()
        .zip(1..)
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    println!("result: {:?}", best);
    Ok(())
}
