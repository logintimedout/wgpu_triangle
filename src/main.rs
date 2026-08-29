use std::sync::Arc;

use anyhow::Ok;
use bytemuck::{Pod, Zeroable};
use wgpu::{
    BackendOptions, Buffer, InstanceFlags, MemoryBudgetThresholds, RenderPipeline, util::DeviceExt,
};
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::{Window, WindowBuilder},
};

//Yea I added excessive comments cause this is mostly a learning project from a toturial I followed

struct State<'window> {
    //Surface/Area of the drawing pallet
    surface: wgpu::Surface<'window>,
    //Direct device connection to the GPU
    device: wgpu::Device,
    //Queue to submit commands
    queue: wgpu::Queue,
    //Configuration for the Surface
    config: wgpu::SurfaceConfiguration,
    //Window Size
    size: winit::dpi::PhysicalSize<u32>,
    //Shared handle to the OS window
    window: Arc<Window>,
    //Render Pipeline
    render_pipeline: RenderPipeline,
    //Triangle Buffer
    vertex_buffer: Buffer,
    num_vertices: u32,
}

impl<'window> State<'window> {
    async fn new(window: Arc<Window>) -> anyhow::Result<State<'window>> {
        //current size in physical pixels
        let size = window.inner_size();

        //Instance is wgpu's entry point it discovers GPU's on the machine itself,
        //Backends::all() attempts to detect all gpu backend's like Vulkan,DirectX,OpenGL
        //since WGPU is basically a wrapper around other graphics apis
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::all(),
            flags: InstanceFlags::default(),
            memory_budget_thresholds: MemoryBudgetThresholds::default(),
            backend_options: BackendOptions::default(),
            //TODO
            display: None,
        });

        //Create surface area, Try implemented
        let surface = instance.create_surface(window.clone())?;

        //Requesting to get Adapter
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptionsBase {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await?;

        //Opening the device gives the Device to acess the GPU and the Queue for submitting work
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                memory_hints: wgpu::MemoryHints::default(),
                trace: wgpu::Trace::Off,
            })
            .await?;

        //Query what the surface supports, then prefer an SRGB colour format
        // so colours display correctly
        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(|f| f.is_srgb())
            .unwrap_or(surface_caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0], //like vsync
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&device, &config);
        let shader: &str = include_str!("shader.wgsl");

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Shader"),
            source: wgpu::ShaderSource::Wgsl(shader.into()),
        });

        let render_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Render Pipeline Layout"),
                bind_group_layouts: &[],
                immediate_size: 0,
            });

        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&render_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::desc()],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Vertex Buffer"),
            contents: bytemuck::cast_slice(VERTICES),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let num_vertices = VERTICES.len() as u32;

        Ok(State {
            surface,
            device,
            queue,
            config,
            size,
            window,
            render_pipeline,
            vertex_buffer,
            num_vertices,
        })
    }

    //Window Resize
    fn resize(&mut self, new_size: winit::dpi::PhysicalSize<u32>) {
        if new_size.width > 0 && new_size.height > 0 {
            self.size = new_size;
            self.config.width = new_size.width;
            self.config.height = new_size.height;
            self.surface.configure(&self.device, &self.config);
        }
    }

    //Draws a single frame, no return value
    fn render(&mut self) {
        let output = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture)
            | wgpu::CurrentSurfaceTexture::Suboptimal(texture) => texture,
            //Skip Frame if timed out
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => return,
            //Surface needs reconfiguring, reapply the current config then skip
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            wgpu::CurrentSurfaceTexture::Validation => return,
        };

        //The view is the handle render passes use to acess the texture's memory
        let view = output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        //The encoder records gpu commands on the CPU side before they are submitted
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("Render Encoder"),
            });

        //Uses an inner scope so the render pass borrows 'encoder' Dropping it at the end
        // of this block releases that borrow so it calls encoder.finish() afterwards
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        //Clear the whole texture to this colour at the start
                        // idk why just following the toturial
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.1,
                            g: 0.2,
                            b: 0.3,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_vertex_buffer(
                0,
                self.vertex_buffer
                    .slice(0..self.num_vertices as u64 * std::mem::size_of::<Vertex>() as u64),
            );
            render_pass.draw(0..self.num_vertices, 0..1);
        };

        //Finish recording and submit the command list to the gpu queue
        // std::iter::once wraps the single command buffer in an iterator
        self.queue.submit(std::iter::once(encoder.finish()));
        output.present();
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
}

const VERTICES: &[Vertex] = &[
    Vertex {
        position: [0.0, 0.5, 0.0],
        color: [1.0, 0.0, 0.0],
    },
    Vertex {
        position: [-0.5, -0.5, 0.0],
        color: [45.0, 1.0, 0.0],
    },
    Vertex {
        position: [0.5, -0.5, 0.0],
        color: [7.0, 0.0, 1.0],
    },
];

impl Vertex {
    fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                //Notes: I think VertexFormat is Float32x3 because of Vertex Struct using fixed size 3 element f32 arrays
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init(); //route wgpu's internal logging through env_logger
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll); //Keep looping even with no events

    //Build the window and wrap it in an ARC for thread safe ownership, for the loop closure
    let window = Arc::new(
        WindowBuilder::new()
            .with_title("Wgpu Demo")
            .with_inner_size(LogicalSize::new(800u32, 600u32))
            .build(&event_loop)?,
    );

    //State::new is async, block on drives it to completion on this thread
    //main can stay an ordinary, non async function
    let mut state = pollster::block_on(State::new(window.clone()))?;

    event_loop.run(move |event, elwt| match event {
        // Only handle events belonging to our window.
        Event::WindowEvent { event, window_id } if window_id == state.window.id() => {
            match event {
                WindowEvent::CloseRequested => elwt.exit(), // user closed the window
                WindowEvent::Resized(physical_size) => state.resize(physical_size),
                // render() handles surface errors internally now, so we just call it.
                WindowEvent::RedrawRequested => state.render(),
                _ => {} // ignore all other window events
            }
        }
        // Fires once every pending event is handled — request the next frame.
        Event::AboutToWait => state.window.request_redraw(),
        _ => {} // ignore non-window events
    })?;

    Ok(())
}
