use std::mem;
use std::sync::Arc;

use ash::version::DeviceV1_0;
use ash::vk;
use specs::shred::PanicHandler;
use specs::{Join, Read, ReadStorage};
use vk_shader_macros::include_glsl;

const SPRITE_VERT: &[u32] = include_glsl!("shaders/sprite.vert");
const SPRITE_FRAG: &[u32] = include_glsl!("shaders/sprite.frag");

use crate::{
    defer,
    graphics::Graphics,
    sim::{Collider, CollisionWorld},
    state::Camera,
};

pub struct Render {
    gfx: Arc<Graphics>,
    pipeline_layout: vk::PipelineLayout,
    render_pass: vk::RenderPass,
    pipeline: vk::Pipeline,
    pool: vk::CommandPool,
    cmd: vk::CommandBuffer,
    viewport: vk::Viewport,
    scissors: vk::Rect2D,
    framebuffers: Vec<vk::Framebuffer>,
    fb_index: u32,
}

impl Drop for Render {
    fn drop(&mut self) {
        let device = &*self.gfx.device;
        unsafe {
            device.destroy_pipeline_layout(self.pipeline_layout, None);
            device.destroy_render_pass(self.render_pass, None);
            device.destroy_pipeline(self.pipeline, None);
            device.destroy_command_pool(self.pool, None);
            for &fb in &self.framebuffers {
                device.destroy_framebuffer(fb, None);
            }
        }
    }
}

impl<'a> specs::System<'a> for Render {
    type SystemData = (
        Read<'a, CollisionWorld, PanicHandler>,
        Read<'a, Camera, PanicHandler>,
        ReadStorage<'a, Collider>,
    );

    fn run(&mut self, (collision, camera, colliders): Self::SystemData) {
        let projection = na::Affine2::from_matrix_unchecked(na::Matrix3::new_nonuniform_scaling(
            &na::Vector2::new(2.0 / self.viewport.width, -2.0 / self.viewport.height),
        ));
        let viewproj = projection * camera.0.inverse();

        let d = &*self.gfx.device;
        let cmd = self.cmd;
        unsafe {
            d.begin_command_buffer(
                cmd,
                &vk::CommandBufferBeginInfo::builder()
                    .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
            )
            .unwrap();
            d.cmd_set_viewport(cmd, 0, &[self.viewport]);
            d.cmd_set_scissor(cmd, 0, &[self.scissors]);

            d.cmd_begin_render_pass(
                cmd,
                &vk::RenderPassBeginInfo::builder()
                    .render_pass(self.render_pass)
                    .framebuffer(self.framebuffers[self.fb_index as usize])
                    .render_area(self.scissors)
                    .clear_values(&[vk::ClearValue {
                        color: vk::ClearColorValue {
                            float32: [0.0, 0.0, 0.0, 0.0],
                        },
                    }]),
                vk::SubpassContents::INLINE,
            );

            d.cmd_bind_pipeline(cmd, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            for collider in (&colliders).join() {
                let collider = collision
                    .collision_object(collider.0)
                    .expect("collider lifetime desync");
                let transform = viewproj * collider.position();
                d.cmd_push_constants(
                    cmd,
                    self.pipeline_layout,
                    vk::ShaderStageFlags::VERTEX,
                    0,
                    &mem::transmute::<_, [u8; 56]>(SpriteParams {
                        transform: transform.to_homogeneous().insert_row(3, 0.0),
                        dimensions: na::Vector2::new(4.0, 4.0),
                    }),
                );
                d.cmd_draw(cmd, 4, 1, 0, 0);
            }

            d.cmd_end_render_pass(cmd);

            d.end_command_buffer(cmd).unwrap();
        }
    }
}

#[repr(C)]
struct SpriteParams {
    transform: na::Matrix4x3<f32>,
    dimensions: na::Vector2<f32>,
}

impl Render {
    pub fn new(gfx: Arc<Graphics>) -> Self {
        let device = &*gfx.device;
        unsafe {
            let sprite_vert = device
                .create_shader_module(
                    &vk::ShaderModuleCreateInfo::builder().code(&SPRITE_VERT),
                    None,
                )
                .unwrap();
            let sv_guard = defer(|| device.destroy_shader_module(sprite_vert, None));

            let sprite_frag = device
                .create_shader_module(
                    &vk::ShaderModuleCreateInfo::builder().code(&SPRITE_FRAG),
                    None,
                )
                .unwrap();
            let sf_guard = defer(|| device.destroy_shader_module(sprite_frag, None));

            let pipeline_layout = device
                .create_pipeline_layout(
                    &vk::PipelineLayoutCreateInfo::builder().push_constant_ranges(&[
                        vk::PushConstantRange {
                            stage_flags: vk::ShaderStageFlags::VERTEX,
                            offset: 0,
                            size: mem::size_of::<SpriteParams>() as u32,
                        },
                    ]),
                    None,
                )
                .unwrap();

            let render_pass = device
                .create_render_pass(
                    &vk::RenderPassCreateInfo::builder()
                        .attachments(&[vk::AttachmentDescription {
                            format: vk::Format::B8G8R8A8_SRGB,
                            samples: vk::SampleCountFlags::TYPE_1,
                            load_op: vk::AttachmentLoadOp::CLEAR,
                            store_op: vk::AttachmentStoreOp::STORE,
                            initial_layout: vk::ImageLayout::UNDEFINED,
                            final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                            ..Default::default()
                        }])
                        .subpasses(&[vk::SubpassDescription::builder()
                            .color_attachments(&[vk::AttachmentReference {
                                attachment: 0,
                                layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            }])
                            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
                            .build()])
                        .dependencies(&[vk::SubpassDependency {
                            src_subpass: vk::SUBPASS_EXTERNAL,
                            dst_subpass: 0,
                            src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                            dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                            dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_READ
                                | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
                            ..Default::default()
                        }]),
                    None,
                )
                .unwrap();

            let entry_point = b"main\0".as_ptr() as *const i8;
            let noop_stencil_state = vk::StencilOpState {
                fail_op: vk::StencilOp::KEEP,
                pass_op: vk::StencilOp::KEEP,
                depth_fail_op: vk::StencilOp::KEEP,
                compare_op: vk::CompareOp::ALWAYS,
                compare_mask: 0,
                write_mask: 0,
                reference: 0,
            };
            let mut pipelines = device
                .create_graphics_pipelines(
                    gfx.pipeline_cache,
                    &[vk::GraphicsPipelineCreateInfo::builder()
                        .stages(&[
                            vk::PipelineShaderStageCreateInfo {
                                stage: vk::ShaderStageFlags::VERTEX,
                                module: sprite_vert,
                                p_name: entry_point,
                                ..Default::default()
                            },
                            vk::PipelineShaderStageCreateInfo {
                                stage: vk::ShaderStageFlags::FRAGMENT,
                                module: sprite_frag,
                                p_name: entry_point,
                                ..Default::default()
                            },
                        ])
                        .vertex_input_state(&Default::default())
                        .input_assembly_state(
                            &vk::PipelineInputAssemblyStateCreateInfo::builder()
                                .topology(vk::PrimitiveTopology::TRIANGLE_STRIP),
                        )
                        .viewport_state(
                            &vk::PipelineViewportStateCreateInfo::builder()
                                .scissor_count(1)
                                .viewport_count(1),
                        )
                        .rasterization_state(
                            &vk::PipelineRasterizationStateCreateInfo::builder()
                                .cull_mode(vk::CullModeFlags::NONE)
                                .polygon_mode(vk::PolygonMode::FILL)
                                .line_width(1.0),
                        )
                        .multisample_state(
                            &vk::PipelineMultisampleStateCreateInfo::builder()
                                .rasterization_samples(vk::SampleCountFlags::TYPE_1),
                        )
                        .depth_stencil_state(
                            &vk::PipelineDepthStencilStateCreateInfo::builder()
                                .depth_test_enable(false)
                                .front(noop_stencil_state)
                                .back(noop_stencil_state),
                        )
                        .color_blend_state(
                            &vk::PipelineColorBlendStateCreateInfo::builder().attachments(&[
                                vk::PipelineColorBlendAttachmentState {
                                    blend_enable: vk::TRUE,
                                    src_color_blend_factor: vk::BlendFactor::ONE,
                                    dst_color_blend_factor: vk::BlendFactor::ZERO,
                                    color_blend_op: vk::BlendOp::ADD,
                                    src_alpha_blend_factor: vk::BlendFactor::ONE,
                                    dst_alpha_blend_factor: vk::BlendFactor::ZERO,
                                    alpha_blend_op: vk::BlendOp::ADD,
                                    color_write_mask: vk::ColorComponentFlags::all(),
                                },
                            ]),
                        )
                        .dynamic_state(
                            &vk::PipelineDynamicStateCreateInfo::builder().dynamic_states(&[
                                vk::DynamicState::VIEWPORT,
                                vk::DynamicState::SCISSOR,
                            ]),
                        )
                        .layout(pipeline_layout)
                        .render_pass(render_pass)
                        .subpass(0)
                        .build()],
                    None,
                )
                .unwrap()
                .into_iter();
            drop((sv_guard, sf_guard));

            let pipeline = pipelines.next().unwrap();

            let pool = gfx
                .device
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::builder()
                        .flags(
                            vk::CommandPoolCreateFlags::TRANSIENT
                                | vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER,
                        )
                        .queue_family_index(gfx.queue_family),
                    None,
                )
                .unwrap();
            let cmd = gfx
                .device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::builder()
                        .command_pool(pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .unwrap()
                .into_iter()
                .next()
                .unwrap();
            Self {
                gfx,
                pipeline_layout,
                render_pass,
                pipeline,
                pool,
                cmd,
                viewport: Default::default(),
                scissors: Default::default(),
                framebuffers: vec![],
                fb_index: 0,
            }
        }
    }

    pub fn cmd(&self) -> vk::CommandBuffer {
        self.cmd
    }

    pub fn set_scissors(&mut self, scissors: vk::Rect2D) {
        self.scissors = scissors;
    }

    pub fn set_viewport(&mut self, viewport: vk::Viewport) {
        self.viewport = viewport;
    }

    /// Recreate framebuffers for a new set of image views
    ///
    /// # Safety
    /// - Must not be called while rendering is in progress
    /// - Must be passed valid ImageViews that will outlive rendering done using them
    pub unsafe fn rebuild_framebuffers(
        &mut self,
        extent: vk::Extent2D,
        views: impl IntoIterator<Item = vk::ImageView>,
    ) {
        let fbs = views
            .into_iter()
            .map(|view| {
                self.gfx.device.create_framebuffer(
                    &vk::FramebufferCreateInfo::builder()
                        .render_pass(self.render_pass)
                        .attachments(&[view])
                        .width(extent.width)
                        .height(extent.height)
                        .layers(1),
                    None,
                )
            })
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        for &fb in &self.framebuffers {
            self.gfx.device.destroy_framebuffer(fb, None);
        }
        self.framebuffers = fbs;
    }

    /// Set the index of the framebuffer to use on the next pass
    ///
    /// # Safety
    /// - Must not currently be in use
    pub unsafe fn set_fb_index(&mut self, index: u32) {
        self.fb_index = index;
    }
}
