use vulkano::image::ImageViewAccess;

pub fn print_image_usage<I>(img: &I)
where
    I: ImageViewAccess,
{
    let unsafe_img = ImageViewAccess::inner(&img);

    let color_attch = unsafe_img.usage_color_attachment();
    let depth_stencil_attch = unsafe_img.usage_depth_stencil_attachment();
    let transient_attch = unsafe_img.usage_transient_attachment();
    let input_attch = unsafe_img.usage_input_attachment();

    let transfer_src = unsafe_img.usage_transfer_source();
    let transfer_dst = unsafe_img.usage_transfer_destination();
    let sampled = unsafe_img.usage_sampled();
    let storage = unsafe_img.usage_storage();

    println!("        --- Image Usage ---");
    println!(" -         Color Attach. - {}", color_attch);
    println!(" - Depth/Stencil Attach. - {}", depth_stencil_attch);
    println!(" -     Transient Attach. - {}", transient_attch);
    println!(" -         Input Attach. - {}", input_attch);

    println!(" -         Transfer Src  - {}", transfer_src);
    println!(" -         Transfer Dst  - {}", transfer_dst);
    println!(" -               Sampled - {}", sampled);
    println!(" -               Storage - {}", storage);
}
