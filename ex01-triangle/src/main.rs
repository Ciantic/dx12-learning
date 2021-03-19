use bindings::{
    windows::win32::direct3d11::*, windows::win32::direct3d12::*, windows::win32::direct3d_hlsl::*,
    windows::win32::direct_composition::*, windows::win32::display_devices::*,
    windows::win32::dxgi::*, windows::win32::gdi::*, windows::win32::menus_and_resources::*,
    windows::win32::system_services::*, windows::win32::windows_and_messaging::*,
};
use dx12_common::create_upload_buffer;
use std::ptr::null_mut;
use std::{convert::TryInto, ffi::CString};
use windows::{Abi, Interface};

const NUM_OF_FRAMES: usize = 2;

#[derive(Debug, PartialEq)]
#[repr(C)]
struct Vertex {
    position: [f32; 3],
    color: [f32; 4],
}
impl Vertex {
    const fn new(position: [f32; 3], color: [f32; 4]) -> Self {
        Self { position, color }
    }
}

// pub fn create_default_buffer() {

// }

#[allow(dead_code)]
struct Window {
    hwnd: HWND,
    factory: IDXGIFactory4,
    adapter: IDXGIAdapter1,
    device: ID3D12Device,
    queue: ID3D12CommandQueue,
    allocators: [ID3D12CommandAllocator; NUM_OF_FRAMES],
    comp_device: IDCompositionDevice,
    swap_chain: IDXGISwapChain3,
    current_frame: usize,
    comp_target: IDCompositionTarget,
    comp_visual: IDCompositionVisual,
    rtv_desc_heap: ID3D12DescriptorHeap,
    rtv_desc_size: usize,
    resources: [ID3D12Resource; NUM_OF_FRAMES],
    root_signature: ID3D12RootSignature,
    list: ID3D12GraphicsCommandList,
    vertex_shader: ID3DBlob,
    pixel_shader: ID3DBlob,
    pipeline_state: ID3D12PipelineState,
    viewport: D3D12_VIEWPORT,
    scissor: RECT,

    // Synchronization
    fence: ID3D12Fence,
    fence_event: HANDLE,
    fence_values: [u64; NUM_OF_FRAMES],

    // Resources
    vertex_buffer: ID3D12Resource,
    vertex_buffer_view: D3D12_VERTEX_BUFFER_VIEW,
}

impl Window {
    pub fn new(hwnd: HWND) -> windows::Result<Self> {
        // Start "DebugView" to listen errors
        // https://docs.microsoft.com/en-us/sysinternals/downloads/debugview
        let debug = unsafe {
            let mut ptr: Option<ID3D12Debug> = None;
            D3D12GetDebugInterface(&ID3D12Debug::IID, ptr.set_abi()).and_some(ptr)
        }
        .expect("Unable to create debug layer");

        unsafe {
            debug.EnableDebugLayer();
        }

        let factory = unsafe {
            let mut ptr: Option<IDXGIFactory4> = None;
            CreateDXGIFactory2(0, &IDXGIFactory4::IID, ptr.set_abi()).and_some(ptr)
        }?;

        let adapter = (0..99)
            .into_iter()
            .find_map(|i| unsafe {
                let mut ptr: Option<IDXGIAdapter1> = None;
                factory.EnumAdapters1(i, &mut ptr).and_some(ptr).ok()
            })
            .expect("Could not find d3d adapter");

        let device = unsafe {
            let mut ptr: Option<ID3D12Device> = None;
            D3D12CreateDevice(
                &adapter, // None for default adapter
                D3D_FEATURE_LEVEL::D3D_FEATURE_LEVEL_11_0,
                &ID3D12Device::IID,
                ptr.set_abi(),
            )
            .and_some(ptr)
        }?;

        let queue = unsafe {
            let mut ptr: Option<ID3D12CommandQueue> = None;
            let desc = D3D12_COMMAND_QUEUE_DESC {
                r#type: D3D12_COMMAND_LIST_TYPE::D3D12_COMMAND_LIST_TYPE_DIRECT,
                priority: D3D12_COMMAND_QUEUE_PRIORITY::D3D12_COMMAND_QUEUE_PRIORITY_HIGH.0,
                flags: D3D12_COMMAND_QUEUE_FLAGS::D3D12_COMMAND_QUEUE_FLAG_NONE,
                node_mask: 0,
            };
            device
                .CreateCommandQueue(&desc, &ID3D12CommandQueue::IID, ptr.set_abi())
                .and_some(ptr)
        }?;

        let allocators: [ID3D12CommandAllocator; NUM_OF_FRAMES] = (0..NUM_OF_FRAMES)
            .map(|_| unsafe {
                let mut ptr: Option<ID3D12CommandAllocator> = None;
                device
                    .CreateCommandAllocator(
                        D3D12_COMMAND_LIST_TYPE::D3D12_COMMAND_LIST_TYPE_DIRECT,
                        &ID3D12CommandAllocator::IID,
                        ptr.set_abi(),
                    )
                    .and_some(ptr)
                    .expect("Unable to create allocator")
            })
            .collect::<Vec<_>>()
            .try_into()
            .expect("Unable to create allocators");

        // Composition device
        let comp_device = unsafe {
            let mut ptr: Option<IDCompositionDevice> = None;
            DCompositionCreateDevice(None, &IDCompositionDevice::IID, ptr.set_abi()).and_some(ptr)
        }?;

        // Create swap chain for composition
        let swap_chain = unsafe {
            let desc = DXGI_SWAP_CHAIN_DESC1 {
                alpha_mode: DXGI_ALPHA_MODE::DXGI_ALPHA_MODE_PREMULTIPLIED,
                buffer_count: NUM_OF_FRAMES as _,
                width: 1024,
                height: 1024,
                format: DXGI_FORMAT::DXGI_FORMAT_B8G8R8A8_UNORM,
                flags: 0,
                buffer_usage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                sample_desc: DXGI_SAMPLE_DESC {
                    count: 1,
                    quality: 0,
                },
                scaling: DXGI_SCALING::DXGI_SCALING_STRETCH,
                stereo: BOOL(1),
                swap_effect: DXGI_SWAP_EFFECT::DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
            };
            let mut ptr: Option<IDXGISwapChain1> = None;
            factory
                .CreateSwapChainForComposition(&queue, &desc, None, &mut ptr)
                .and_some(ptr)
        }?
        .cast::<IDXGISwapChain3>()?;

        // Current frame index
        let current_frame = unsafe { swap_chain.GetCurrentBackBufferIndex() as usize };

        // Create IDCompositionTarget for the window
        let comp_target = unsafe {
            let mut ptr = None;
            comp_device
                .CreateTargetForHwnd(hwnd, BOOL(1), &mut ptr)
                .and_some(ptr)
        }?;

        // Create IDCompositionVisual for the window
        let comp_visual = unsafe {
            let mut ptr = None;
            comp_device.CreateVisual(&mut ptr).and_some(ptr)
        }?;

        // Set swap_chain and the root visual and commit
        unsafe {
            comp_visual.SetContent(&swap_chain).ok()?;
            comp_target.SetRoot(&comp_visual).ok()?;
            comp_device.Commit().ok()?;
        }

        // Create descriptor heap for render target views
        let rtv_desc_heap = unsafe {
            let desc = D3D12_DESCRIPTOR_HEAP_DESC {
                r#type: D3D12_DESCRIPTOR_HEAP_TYPE::D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                num_descriptors: NUM_OF_FRAMES as _,
                flags: D3D12_DESCRIPTOR_HEAP_FLAGS::D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                node_mask: 0,
            };
            let mut ptr: Option<ID3D12DescriptorHeap> = None;
            device
                .CreateDescriptorHeap(&desc, &ID3D12DescriptorHeap::IID, ptr.set_abi())
                .and_some(ptr)
        }?;

        // Create resource per frame
        let mut descriptor = unsafe { rtv_desc_heap.GetCPUDescriptorHandleForHeapStart() };
        let rtv_desc_size = unsafe {
            device.GetDescriptorHandleIncrementSize(
                D3D12_DESCRIPTOR_HEAP_TYPE::D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
            ) as usize
        };
        let resources = (0..NUM_OF_FRAMES)
            .map(|i| {
                let resource = unsafe {
                    let mut ptr: Option<ID3D12Resource> = None;
                    swap_chain
                        .GetBuffer(i as _, &ID3D12Resource::IID, ptr.set_abi())
                        .and_some(ptr)
                }?;

                unsafe {
                    // let desc = D3D12_TEX2D_RTV {
                    //     Format: DXGI_FORMAT_R8G8B8A8_UNORM,
                    //     u: D3D12_RTV_DIMENSION_UNKNOWN as _,
                    //     ViewDimension: 0,
                    // };
                    device.CreateRenderTargetView(&resource, 0 as _, &descriptor);
                    descriptor.ptr += rtv_desc_size;
                }

                Ok(resource)
            })
            .collect::<Result<Vec<_>, windows::ErrorCode>>()?
            .try_into()
            .expect("Unable to create resources");

        // Create root signature
        let root_signature = unsafe {
            let root = {
                let mut blob: Option<ID3DBlob> = None;
                let mut error: Option<ID3DBlob> = None;

                let desc = D3D12_ROOT_SIGNATURE_DESC {
                    num_parameters: 0,
                    p_parameters: null_mut() as _,
                    num_static_samplers: 0,
                    p_static_samplers: null_mut() as _,
                    flags: D3D12_ROOT_SIGNATURE_FLAGS::D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
                };
                D3D12SerializeRootSignature(
                    &desc,
                    D3D_ROOT_SIGNATURE_VERSION::D3D_ROOT_SIGNATURE_VERSION_1_0,
                    &mut blob as _,
                    &mut error as _,
                )
                .and_then(|| {
                    if error.is_none() {
                        blob.unwrap()
                    } else {
                        panic!("Root signature failed, error blob contains the error")
                    }
                })
            }?;

            let mut ptr: Option<ID3D12RootSignature> = None;
            device
                .CreateRootSignature(
                    0,
                    root.GetBufferPointer(),
                    root.GetBufferSize(),
                    &ID3D12RootSignature::IID,
                    ptr.set_abi(),
                )
                .and_some(ptr)
        }?;

        let vertex_shader = unsafe {
            let data = include_bytes!("./simple.hlsl");
            let mut err: Option<ID3DBlob> = None;
            let mut ptr: Option<ID3DBlob> = None;

            D3DCompile(
                data.as_ptr() as *mut _,
                data.len(),
                PSTR("simple.hlsl\0".as_ptr() as _),
                null_mut(),
                None,
                PSTR("VSMain\0".as_ptr() as _),
                PSTR("vs_5_0\0".as_ptr() as _),
                0,
                0,
                &mut ptr,
                &mut err,
            )
            .ok()?;

            match ptr {
                Some(v) => v,
                None => {
                    panic!(
                        "Shader creation failed with error {}",
                        CString::from_raw(err.unwrap().GetBufferPointer() as _).to_string_lossy()
                    )
                }
            }
        };

        let pixel_shader = unsafe {
            let data = include_bytes!("./simple.hlsl");
            let mut err: Option<ID3DBlob> = None;
            let mut ptr: Option<ID3DBlob> = None;

            D3DCompile(
                data.as_ptr() as *mut _,
                data.len(),
                PSTR("simple.hlsl\0".as_ptr() as _),
                null_mut(),
                None,
                PSTR("PSMain\0".as_ptr() as _),
                PSTR("ps_5_0\0".as_ptr() as _),
                0,
                0,
                &mut ptr,
                &mut err,
            )
            .ok()?;

            match ptr {
                Some(v) => v,
                None => {
                    panic!(
                        "Shader creation failed with error {}",
                        CString::from_raw(err.unwrap().GetBufferPointer() as _).to_string_lossy()
                    )
                }
            }
        };

        let mut els = [
            D3D12_INPUT_ELEMENT_DESC {
                semantic_name: PSTR("POSITION\0".as_ptr() as _),
                semantic_index: 0,
                format: DXGI_FORMAT::DXGI_FORMAT_R32G32B32_FLOAT,
                input_slot: 0,
                instance_data_step_rate: 0,
                input_slot_class:
                    D3D12_INPUT_CLASSIFICATION::D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                aligned_byte_offset: 0,
            },
            D3D12_INPUT_ELEMENT_DESC {
                semantic_name: PSTR("COLOR\0".as_ptr() as _),
                semantic_index: 0,
                format: DXGI_FORMAT::DXGI_FORMAT_R32G32B32A32_FLOAT,
                input_slot: 0,
                instance_data_step_rate: 0,
                input_slot_class:
                    D3D12_INPUT_CLASSIFICATION::D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                aligned_byte_offset: 12,
            },
        ];

        let pso_desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            // TODO: Can I get rid of this clone? Or do I even have to?
            p_root_signature: Some(root_signature.clone()),
            // unsafe { std::mem::transmute(root_signature.abi()) },
            input_layout: D3D12_INPUT_LAYOUT_DESC {
                num_elements: els.len() as u32,
                p_input_element_descs: els.as_mut_ptr(),
            },
            vs: D3D12_SHADER_BYTECODE {
                bytecode_length: unsafe { vertex_shader.GetBufferSize() },
                p_shader_bytecode: unsafe { vertex_shader.GetBufferPointer() },
            },
            ps: D3D12_SHADER_BYTECODE {
                bytecode_length: unsafe { pixel_shader.GetBufferSize() },
                p_shader_bytecode: unsafe { pixel_shader.GetBufferPointer() },
            },
            // CD3DX12_RASTERIZER_DESC( CD3DX12_DEFAULT )
            rasterizer_state: D3D12_RASTERIZER_DESC {
                fill_mode: D3D12_FILL_MODE::D3D12_FILL_MODE_SOLID,
                cull_mode: D3D12_CULL_MODE::D3D12_CULL_MODE_BACK,
                front_counter_clockwise: BOOL(0),
                depth_bias: D3D12_DEFAULT_DEPTH_BIAS as _,
                depth_bias_clamp: D3D12_DEFAULT_DEPTH_BIAS_CLAMP,
                slope_scaled_depth_bias: D3D12_DEFAULT_SLOPE_SCALED_DEPTH_BIAS,
                depth_clip_enable: BOOL(1),
                multisample_enable: BOOL(0),
                antialiased_line_enable: BOOL(0),
                forced_sample_count: 0,
                conservative_raster:
                    D3D12_CONSERVATIVE_RASTERIZATION_MODE::D3D12_CONSERVATIVE_RASTERIZATION_MODE_OFF,
            },
            // CD3DX12_BLEND_DESC(D3D12_DEFAULT)
            blend_state: D3D12_BLEND_DESC {
                alpha_to_coverage_enable: BOOL(0),
                independent_blend_enable: BOOL(0),
                render_target: (0..D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT)
                    .map(|_| D3D12_RENDER_TARGET_BLEND_DESC {
                        blend_enable: false.into(),
                        logic_op_enable: false.into(),
                        dest_blend: D3D12_BLEND::D3D12_BLEND_ZERO,
                        src_blend: D3D12_BLEND::D3D12_BLEND_ZERO,
                        dest_blend_alpha: D3D12_BLEND::D3D12_BLEND_ONE,
                        src_blend_alpha: D3D12_BLEND::D3D12_BLEND_ONE,
                        blend_op: D3D12_BLEND_OP::D3D12_BLEND_OP_ADD,
                        logic_op: D3D12_LOGIC_OP::D3D12_LOGIC_OP_NOOP,
                        blend_op_alpha: D3D12_BLEND_OP::D3D12_BLEND_OP_ADD,
                        render_target_write_mask:
                            D3D12_COLOR_WRITE_ENABLE::D3D12_COLOR_WRITE_ENABLE_ALL.0 as _,
                    })
                    .collect::<Vec<_>>()
                    .as_slice()
                    .try_into()
                    .unwrap(),
            },
            sample_mask: 0xffffffff,
            primitive_topology_type:
                D3D12_PRIMITIVE_TOPOLOGY_TYPE::D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            num_render_targets: 1,
            rtv_formats: (0..D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT)
                .map(|i| {
                    if i == 0 {
                        DXGI_FORMAT::DXGI_FORMAT_B8G8R8A8_UNORM
                    } else {
                        DXGI_FORMAT::DXGI_FORMAT_UNKNOWN
                    }
                })
                .collect::<Vec<_>>()
                .try_into()
                .unwrap(),
            sample_desc: DXGI_SAMPLE_DESC {
                count: 1,
                quality: 0,
            },
            ..D3D12_GRAPHICS_PIPELINE_STATE_DESC::default()
        };

        let pipeline_state = unsafe {
            let mut ptr: Option<ID3D12PipelineState> = None;
            device
                .CreateGraphicsPipelineState(&pso_desc, &ID3D12PipelineState::IID, ptr.set_abi())
                .and_some(ptr)
        }
        .expect("Unable to create pipeline state");

        // Create direct command list
        let list = unsafe {
            let mut ptr: Option<ID3D12GraphicsCommandList> = None;
            device
                .CreateCommandList(
                    0,
                    D3D12_COMMAND_LIST_TYPE::D3D12_COMMAND_LIST_TYPE_DIRECT,
                    &allocators[current_frame],
                    &pipeline_state,
                    &ID3D12GraphicsCommandList::IID,
                    ptr.set_abi(),
                )
                .and_then(|| {
                    let ptr = ptr.unwrap();
                    ptr.Close().unwrap();
                    ptr
                })
        }?;

        let (vertex_buffer, vertex_buffer_view) = unsafe {
            // Blue end of the triangle is semi transparent
            let ar = 1.0;
            let scale = 1.0;
            let cpu_triangle: [Vertex; 3] = [
                Vertex::new([0.0, scale * ar, 0.0], [1.0, 0.0, 0.0, 1.0]),
                Vertex::new([scale, -scale * ar, 0.0], [0.0, 1.0, 0.0, 1.0]),
                Vertex::new([-scale, -scale * ar, 0.0], [0.0, 0.0, 1.0, 0.5]),
            ];
            let triangle_size_bytes = std::mem::size_of_val(&cpu_triangle);

            let cpu_triangle_bytes = std::slice::from_raw_parts(
                (&cpu_triangle as *const _) as *const u8,
                std::mem::size_of_val(&cpu_triangle),
            );

            let vertex_buffer = create_upload_buffer(&device, cpu_triangle_bytes)?;
            let vertex_buffer_view = D3D12_VERTEX_BUFFER_VIEW {
                buffer_location: vertex_buffer.GetGPUVirtualAddress(),
                stride_in_bytes: std::mem::size_of::<Vertex>() as _,
                size_in_bytes: triangle_size_bytes as _,
            };
            (vertex_buffer, vertex_buffer_view)
        };

        // Create fence
        let (fence, fence_values, fence_event) = unsafe {
            let mut ptr: Option<ID3D12Fence> = None;
            let fence = device
                .CreateFence(
                    0,
                    D3D12_FENCE_FLAGS::D3D12_FENCE_FLAG_NONE,
                    &ID3D12Fence::IID,
                    ptr.set_abi(),
                )
                .and_some(ptr)?;
            let fence_event = CreateEventA(null_mut(), false, false, PSTR(null_mut()));
            if fence_event.0 == 0 {
                panic!("Unable to create fence event");
            }
            (fence, [0, 0], fence_event)
        };

        let viewport = D3D12_VIEWPORT {
            width: 1024.0,
            height: 1024.0,
            max_depth: D3D12_MAX_DEPTH,
            min_depth: D3D12_MIN_DEPTH,
            top_leftx: 0.0,
            top_lefty: 0.0,
        };

        let scissor = RECT {
            top: 0,
            left: 0,
            bottom: 1024,
            right: 1024,
        };

        Ok(Window {
            hwnd,
            factory,
            adapter,
            device,
            queue,
            allocators,
            comp_device,
            swap_chain,
            current_frame,
            comp_target,
            comp_visual,
            rtv_desc_heap,
            rtv_desc_size,
            resources,
            root_signature,
            list,
            pipeline_state,
            vertex_shader,
            pixel_shader,
            viewport,
            scissor,
            fence,
            fence_event,
            fence_values,
            vertex_buffer,
            vertex_buffer_view,
        })
    }

    fn populate_command_list(&mut self) -> ::windows::Result<()> {
        unsafe {
            // Get the current backbuffer on which to draw
            let current_frame = self.swap_chain.GetCurrentBackBufferIndex() as usize;
            let current_resource = &self.resources[current_frame];
            let rtv = {
                let mut ptr = self.rtv_desc_heap.GetCPUDescriptorHandleForHeapStart();
                ptr.ptr += self.rtv_desc_size * current_frame;
                ptr
            };

            // Reset allocator
            self.allocators[current_frame].Reset().ok()?;

            // Reset list
            self.list
                .Reset(&self.allocators[current_frame], &self.pipeline_state)
                .ok()?;

            // Set root signature, viewport and scissor rect
            self.list.SetGraphicsRootSignature(&self.root_signature);
            self.list.RSSetViewports(1, &self.viewport);
            self.list.RSSetScissorRects(1, &self.scissor);

            // Direct the draw commands to the render target resource
            let barriers = {
                let mut barrier = D3D12_RESOURCE_BARRIER {
                    r#type: D3D12_RESOURCE_BARRIER_TYPE::D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                    flags: D3D12_RESOURCE_BARRIER_FLAGS::D3D12_RESOURCE_BARRIER_FLAG_NONE,
                    ..std::mem::zeroed()
                };
                barrier.anonymous.transition.subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
                barrier.anonymous.transition.p_resource = current_resource.abi();
                barrier.anonymous.transition.state_before =
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_PRESENT;
                barrier.anonymous.transition.state_after =
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_RENDER_TARGET;
                [barrier]
            };
            self.list.ResourceBarrier(1, barriers.as_ptr());

            self.list.OMSetRenderTargets(1, &rtv, false, null_mut());

            self.list
                .ClearRenderTargetView(rtv, [1.0f32, 0.2, 0.4, 0.5].as_ptr(), 0, null_mut());
            self.list.IASetPrimitiveTopology(
                D3D_PRIMITIVE_TOPOLOGY::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            );
            self.list.IASetVertexBuffers(0, 1, &self.vertex_buffer_view);
            self.list.DrawInstanced(3, 1, 0, 0);

            // Direct the draw commands to the render target resource
            let barriers = {
                let mut barrier = D3D12_RESOURCE_BARRIER {
                    r#type: D3D12_RESOURCE_BARRIER_TYPE::D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
                    flags: D3D12_RESOURCE_BARRIER_FLAGS::D3D12_RESOURCE_BARRIER_FLAG_NONE,
                    ..std::mem::zeroed()
                };
                barrier.anonymous.transition.subresource = D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES;
                barrier.anonymous.transition.p_resource = current_resource.abi();
                barrier.anonymous.transition.state_before =
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_RENDER_TARGET;
                barrier.anonymous.transition.state_after =
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_PRESENT;
                [barrier]
            };
            self.list.ResourceBarrier(1, barriers.as_ptr());

            // Close list
            self.list.Close().ok()?;
            Ok(())
        }
    }

    pub fn wait_for_gpu(&mut self) -> windows::Result<()> {
        unsafe {
            let fence_value = self.fence_values[self.current_frame];
            self.queue.Signal(&self.fence, fence_value).ok()?;
            self.fence
                .SetEventOnCompletion(fence_value, self.fence_event)
                .ok()?;

            WaitForSingleObjectEx(self.fence_event, 0xFFFFFFFF, false);

            self.fence_values[self.current_frame] += 1;
            Ok(())
        }
    }

    pub fn move_to_next_frame(&mut self) -> windows::Result<()> {
        unsafe {
            let current_fence_value = self.fence_values[self.current_frame];
            self.queue.Signal(&self.fence, current_fence_value).ok()?;

            // Update current frame
            self.current_frame = self.swap_chain.GetCurrentBackBufferIndex() as usize;
            let wait_fence_value = self.fence_values[self.current_frame];

            // If the next frame is not ready to be rendered yet, wait until it is ready.
            if self.fence.GetCompletedValue() < wait_fence_value {
                self.fence
                    .SetEventOnCompletion(wait_fence_value, self.fence_event)
                    .ok()?;
                WaitForSingleObjectEx(self.fence_event, 0xFFFFFFFF, false);
            }

            // Update the fence value
            self.fence_values[self.current_frame] = current_fence_value + 1;
            Ok(())
        }
    }

    pub fn render(&mut self) -> windows::Result<()> {
        self.populate_command_list()?;
        unsafe {
            let mut lists = [Some(self.list.cast::<ID3D12CommandList>()?)];
            self.queue
                .ExecuteCommandLists(lists.len() as _, lists.as_mut_ptr());
            self.swap_chain.Present(1, 0).ok()?;
        }
        self.move_to_next_frame()?;
        Ok(())
    }
}

/// Main message loop for the window
extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        static mut WINDOW: Option<Window> = None;
        match msg {
            WM_CREATE => {
                let mut win = Window::new(hwnd).unwrap();
                win.wait_for_gpu().unwrap();
                WINDOW = Some(win);
                DefWindowProcA(hwnd, msg, wparam, lparam)
            }
            WM_PAINT => {
                if let Some(window) = WINDOW.as_mut() {
                    window.render().unwrap();
                }
                ValidateRect(hwnd, std::ptr::null());
                LRESULT(0)
            }
            WM_DESTROY => {
                WINDOW = None;
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcA(hwnd, msg, wparam, lparam),
        }
    }
}

fn main() {
    unsafe {
        let instance = HINSTANCE(GetModuleHandleA(PSTR(null_mut())));
        let cursor = LoadCursorA(HINSTANCE(0), PSTR(IDC_ARROW as _));
        let cls = WNDCLASSA {
            style: 0,
            lpfn_wnd_proc: Some(wndproc),
            h_instance: instance,
            lpsz_class_name: PSTR(b"CompositionCls\0".as_ptr() as _),
            cb_cls_extra: 0,
            cb_wnd_extra: 0,
            h_icon: HICON(0),
            h_cursor: cursor,
            hbr_background: HBRUSH(0),
            lpsz_menu_name: PSTR(null_mut()),
        };
        RegisterClassA(&cls);
        let hwnd = CreateWindowExA(
            WINDOWS_EX_STYLE::WS_EX_NOREDIRECTIONBITMAP as _,
            PSTR(b"CompositionCls\0".as_ptr() as _),
            PSTR(b"Composition example\0".as_ptr() as _),
            WINDOWS_STYLE::WS_OVERLAPPEDWINDOW | WINDOWS_STYLE::WS_VISIBLE,
            -2147483648 as _, // Where is CW_USEDEFAULT? I just hardcoded the value
            -2147483648 as _,
            -2147483648 as _,
            -2147483648 as _,
            HWND(0),
            HMENU(0),
            instance,
            0 as _,
        );
        if hwnd == HWND(0) {
            panic!("Failed to create window");
        }

        let mut message = MSG::default();

        while GetMessageA(&mut message, HWND(0), 0, 0).into() {
            TranslateMessage(&mut message);
            DispatchMessageA(&mut message);
        }
    }
}
