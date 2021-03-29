use bindings::{
    Windows::Win32::Direct3D11::*, Windows::Win32::Direct3D12::*, Windows::Win32::Direct3DHlsl::*,
    Windows::Win32::DirectComposition::*, Windows::Win32::DisplayDevices::*,
    Windows::Win32::Dxgi::*, Windows::Win32::Gdi::*, Windows::Win32::HiDpi::*,
    Windows::Win32::KeyboardAndMouseInput::*, Windows::Win32::MenusAndResources::*,
    Windows::Win32::SystemServices::*, Windows::Win32::WindowsAndMessaging::*,
};

use dx12_common::{
    cd3dx12_blend_desc_default, cd3dx12_depth_stencil_desc_default,
    cd3dx12_heap_properties_with_type, cd3dx12_rasterizer_desc_default,
    cd3dx12_resource_barrier_transition, create_default_buffer,
};
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

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const GREEN: [f32; 4] = [0.0, 1.0, 0.0, 1.0];
const BLUE_TRANSPARENT: [f32; 4] = [0.0, 0.0, 1.0, 0.5];
const MAGENTA: [f32; 4] = [1.0, 0.0, 1.0, 1.0];

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
    back_buffers: [ID3D12Resource; NUM_OF_FRAMES],
    depth_stencil_heap: ID3D12DescriptorHeap,
    depth_stencil_buffer: ID3D12Resource,
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

    indices_buffer: ID3D12Resource,
    indices_buffer_view: D3D12_INDEX_BUFFER_VIEW,
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
                Type: D3D12_COMMAND_LIST_TYPE::D3D12_COMMAND_LIST_TYPE_DIRECT,
                Priority: D3D12_COMMAND_QUEUE_PRIORITY::D3D12_COMMAND_QUEUE_PRIORITY_HIGH.0,
                Flags: D3D12_COMMAND_QUEUE_FLAGS::D3D12_COMMAND_QUEUE_FLAG_NONE,
                NodeMask: 0,
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
                AlphaMode: DXGI_ALPHA_MODE::DXGI_ALPHA_MODE_PREMULTIPLIED,
                BufferCount: NUM_OF_FRAMES as _,
                Width: 1024,
                Height: 1024,
                Format: DXGI_FORMAT::DXGI_FORMAT_B8G8R8A8_UNORM,
                Flags: 0,
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Scaling: DXGI_SCALING::DXGI_SCALING_STRETCH,
                Stereo: BOOL(0),
                SwapEffect: DXGI_SWAP_EFFECT::DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL,
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
                Type: D3D12_DESCRIPTOR_HEAP_TYPE::D3D12_DESCRIPTOR_HEAP_TYPE_RTV,
                NumDescriptors: NUM_OF_FRAMES as _,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAGS::D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                NodeMask: 0,
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
        let back_buffers = (0..NUM_OF_FRAMES)
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

        // Create depth/stencil heap
        let depth_stencil_heap = unsafe {
            let desc = D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE::D3D12_DESCRIPTOR_HEAP_TYPE_DSV,
                NumDescriptors: 1,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAGS::D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                NodeMask: 0,
            };
            let mut ptr: Option<ID3D12DescriptorHeap> = None;
            device
                .CreateDescriptorHeap(&desc, &ID3D12DescriptorHeap::IID, ptr.set_abi())
                .and_some(ptr)
        }?;

        // Create depth/stencil buffer
        let depth_stencil_buffer = unsafe {
            let mut ptr: Option<ID3D12Resource> = None;
            device
                .CreateCommittedResource(
                    &cd3dx12_heap_properties_with_type(D3D12_HEAP_TYPE::D3D12_HEAP_TYPE_DEFAULT),
                    D3D12_HEAP_FLAGS::D3D12_HEAP_FLAG_NONE,
                    &D3D12_RESOURCE_DESC {
                        Alignment: 0,
                        Width: 1024,
                        Height: 1024,

                        // If DXGI_SWAP_CHAIN_DESC1::Stereo is TRUE (3d glasses
                        // support) following array size needs to be 2:
                        DepthOrArraySize: 1,

                        MipLevels: 1,
                        Dimension: D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_TEXTURE2D,
                        SampleDesc: DXGI_SAMPLE_DESC {
                            Count: 1,
                            Quality: 0,
                        },
                        Format: DXGI_FORMAT::DXGI_FORMAT_D32_FLOAT,
                        Flags: D3D12_RESOURCE_FLAGS::D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
                        ..std::mem::zeroed()
                    },
                    // D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_COMMON,
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_DEPTH_WRITE,
                    &D3D12_CLEAR_VALUE {
                        Format: DXGI_FORMAT::DXGI_FORMAT_D32_FLOAT,
                        Anonymous: D3D12_CLEAR_VALUE_0 {
                            DepthStencil: D3D12_DEPTH_STENCIL_VALUE {
                                Depth: 1.0,
                                Stencil: 0,
                            },
                        },
                    },
                    &ID3D12Resource::IID,
                    ptr.set_abi(),
                )
                .and_some(ptr)
        }?;

        unsafe {
            device.CreateDepthStencilView(
                &depth_stencil_buffer,
                null_mut(),
                // &D3D12_DEPTH_STENCIL_VIEW_DESC {
                //     format: DXGI_FORMAT::DXGI_FORMAT_D32_FLOAT,
                //     view_dimension: D3D12_DSV_DIMENSION::D3D12_DSV_DIMENSION_TEXTURE2D,
                //     flags: D3D12_DSV_FLAGS::D3D12_DSV_FLAG_NONE,

                //     ..std::mem::zeroed()
                // },
                depth_stencil_heap.GetCPUDescriptorHandleForHeapStart(),
            )
        }

        // Create root signature
        let root_signature = unsafe {
            let root = {
                let mut blob: Option<ID3DBlob> = None;
                let mut error: Option<ID3DBlob> = None;

                let desc = D3D12_ROOT_SIGNATURE_DESC {
                    NumParameters: 0,
                    pParameters: null_mut() as _,
                    NumStaticSamplers: 0,
                    pStaticSamplers: null_mut() as _,
                    Flags: D3D12_ROOT_SIGNATURE_FLAGS::D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,
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
            let data = include_bytes!("./01-triangle.hlsl");
            let mut err: Option<ID3DBlob> = None;
            let mut ptr: Option<ID3DBlob> = None;

            D3DCompile(
                data.as_ptr() as *mut _,
                data.len(),
                PSTR("01-triangle.hlsl\0".as_ptr() as _),
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
            let data = include_bytes!("./01-triangle.hlsl");
            let mut err: Option<ID3DBlob> = None;
            let mut ptr: Option<ID3DBlob> = None;

            D3DCompile(
                data.as_ptr() as *mut _,
                data.len(),
                PSTR("01-triangle.hlsl\0".as_ptr() as _),
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
                SemanticName: PSTR("POSITION\0".as_ptr() as _),
                SemanticIndex: 0,
                Format: DXGI_FORMAT::DXGI_FORMAT_R32G32B32_FLOAT,
                InputSlot: 0,
                InstanceDataStepRate: 0,
                InputSlotClass:
                    D3D12_INPUT_CLASSIFICATION::D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                AlignedByteOffset: 0,
            },
            D3D12_INPUT_ELEMENT_DESC {
                SemanticName: PSTR("COLOR\0".as_ptr() as _),
                SemanticIndex: 0,
                Format: DXGI_FORMAT::DXGI_FORMAT_R32G32B32A32_FLOAT,
                InputSlot: 0,
                InstanceDataStepRate: 0,
                InputSlotClass:
                    D3D12_INPUT_CLASSIFICATION::D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA,
                AlignedByteOffset: 12,
            },
        ];

        let pso_desc = D3D12_GRAPHICS_PIPELINE_STATE_DESC {
            // TODO: Can I get rid of this clone? Or do I even have to?
            pRootSignature: Some(root_signature.clone()),
            // unsafe { std::mem::transmute(root_signature.abi()) },
            InputLayout: D3D12_INPUT_LAYOUT_DESC {
                NumElements: els.len() as u32,
                pInputElementDescs: els.as_mut_ptr(),
            },
            VS: D3D12_SHADER_BYTECODE {
                BytecodeLength: unsafe { vertex_shader.GetBufferSize() },
                pShaderBytecode: unsafe { vertex_shader.GetBufferPointer() },
            },
            PS: D3D12_SHADER_BYTECODE {
                BytecodeLength: unsafe { pixel_shader.GetBufferSize() },
                pShaderBytecode: unsafe { pixel_shader.GetBufferPointer() },
            },
            RasterizerState: cd3dx12_rasterizer_desc_default(),
            BlendState: cd3dx12_blend_desc_default(),
            SampleMask: 0xffffffff,
            PrimitiveTopologyType:
                D3D12_PRIMITIVE_TOPOLOGY_TYPE::D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE,
            NumRenderTargets: 1,
            RTVFormats: (0..D3D12_SIMULTANEOUS_RENDER_TARGET_COUNT)
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
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            DSVFormat: DXGI_FORMAT::DXGI_FORMAT_D32_FLOAT,
            DepthStencilState: cd3dx12_depth_stencil_desc_default(),
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
                .and_some(ptr)
        }?;
        unsafe {
            list.Close().ok()?;
        }

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
            (fence, [0; NUM_OF_FRAMES], fence_event)
        };

        let viewport = D3D12_VIEWPORT {
            Width: 1024.0,
            Height: 1024.0,
            MaxDepth: D3D12_MAX_DEPTH,
            MinDepth: D3D12_MIN_DEPTH,
            TopLeftX: 0.0,
            TopLeftY: 0.0,
        };

        let scissor = RECT {
            top: 0,
            left: 0,
            bottom: 1024,
            right: 1024,
        };

        // Resource initialization ------------------------------------------
        unsafe {
            // allocators[current_frame].Reset().ok()?;
            list.Reset(&allocators[current_frame], &pipeline_state)
                .ok()?;
        }

        let (vertex_buffer, vertex_buffer_view, _vertex_buffer_upload) = unsafe {
            // Coordinate space again as refresher:
            //
            //    x, y
            // -1.0, +1.0            +1.0, +1.0
            //     0──────────┬──────────1 ◄─── vertex index
            //     │          │          │
            //     │          │          │
            //     │          │          │
            //     │          │          │
            //     │        0,│0         │
            //     ├──────────┼──────────┤
            //     │          │          │
            //     │          │          │
            //     │          │          │
            //     │          │          │
            //     │          │          │
            //     3──────────┴──────────2
            // -1.0, -1.0            +1.0, -1.0

            // In order to create quad (that is square), we form two triangles
            // from the vertices:
            //
            // Indices 0, 1, 2 form a first triangle, and
            // indices 0, 2, 3 form a second triangle.

            // Vertexes (these don't form the triangle, but the indicies do)
            let vertices: [Vertex; 8] = [
                // First
                Vertex::new([-0.5, 0.5, 0.8], RED),
                Vertex::new([0.5, 0.5, 0.8], GREEN),
                Vertex::new([0.5, -0.5, 0.8], BLUE_TRANSPARENT),
                Vertex::new([-0.5, -0.5, 0.8], MAGENTA),
                // Second
                Vertex::new([-0.5 - 0.2, 0.5 - 0.2, 0.7], RED),
                Vertex::new([0.5 - 0.2, 0.5 - 0.2, 0.7], GREEN),
                Vertex::new([0.5 - 0.2, -0.5 - 0.2, 0.7], BLUE_TRANSPARENT),
                Vertex::new([-0.5 - 0.2, -0.5 - 0.2, 0.7], MAGENTA),
            ];

            let vertices_as_bytes = std::slice::from_raw_parts(
                (&vertices as *const _) as *const u8,
                std::mem::size_of_val(&vertices),
            );

            let vertex_buffers = create_default_buffer(&device, &list, vertices_as_bytes)?;

            let vertex_buffer_view = D3D12_VERTEX_BUFFER_VIEW {
                BufferLocation: vertex_buffers.gpu_buffer.GetGPUVirtualAddress(),
                StrideInBytes: std::mem::size_of::<Vertex>() as _,
                SizeInBytes: vertices_as_bytes.len() as _,
            };

            (
                vertex_buffers.gpu_buffer,
                vertex_buffer_view,
                vertex_buffers.upload_buffer,
            )
        };

        let (indices_buffer, indices_buffer_view, _indicies_upload_buffer) = unsafe {
            // Vertex indicies which form the two triangles:
            let indices: [u32; 12] = [
                0, 1, 2, // Upper right triangle
                0, 2, 3, // Bottom left triangle
                4, 5, 6, // Upper right triangle
                4, 6, 7, // Bottom left triangle
            ];

            let indicies_as_bytes = std::slice::from_raw_parts(
                (&indices as *const _) as *const u8,
                std::mem::size_of_val(&indices),
            );

            let buffers = create_default_buffer(&device, &list, indicies_as_bytes)?;

            let view = D3D12_INDEX_BUFFER_VIEW {
                BufferLocation: buffers.gpu_buffer.GetGPUVirtualAddress(),
                SizeInBytes: indicies_as_bytes.len() as _,
                Format: DXGI_FORMAT::DXGI_FORMAT_R32_UINT,
            };

            (buffers.gpu_buffer, view, buffers.upload_buffer)
        };

        unsafe {
            list.Close().ok()?;
            let mut lists = [Some(list.cast::<ID3D12CommandList>()?)];
            queue.ExecuteCommandLists(lists.len() as _, lists.as_mut_ptr());
        }

        let mut win = Window {
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
            back_buffers,
            depth_stencil_heap,
            depth_stencil_buffer,
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
            indices_buffer,
            indices_buffer_view,
        };

        win.wait_for_gpu()?;

        // Temporary upload buffers _indicies_upload_buffer, and
        // _vertex_buffer_upload can now be destroyed.

        // End of resource initialization -------------------------------

        Ok(win)
    }

    fn populate_command_list(&mut self) -> ::windows::Result<()> {
        unsafe {
            // Get the current backbuffer on which to draw
            let current_frame = self.swap_chain.GetCurrentBackBufferIndex() as usize;
            let current_back_buffer = &self.back_buffers[current_frame];
            let rtv = {
                let mut ptr = self.rtv_desc_heap.GetCPUDescriptorHandleForHeapStart();
                ptr.ptr += self.rtv_desc_size * current_frame;
                ptr
            };
            let dsv = self.depth_stencil_heap.GetCPUDescriptorHandleForHeapStart();

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
            self.list.ResourceBarrier(
                1,
                &cd3dx12_resource_barrier_transition(
                    current_back_buffer,
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_PRESENT,
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_RENDER_TARGET,
                    None,
                    None,
                ),
            );
            self.list.ClearDepthStencilView(
                &dsv,
                D3D12_CLEAR_FLAGS::from(
                    D3D12_CLEAR_FLAGS::D3D12_CLEAR_FLAG_DEPTH.0
                        | D3D12_CLEAR_FLAGS::D3D12_CLEAR_FLAG_STENCIL.0,
                ),
                1.0,
                0,
                0,
                null_mut(),
            );
            self.list.OMSetRenderTargets(1, &rtv, false, &dsv);

            self.list
                .ClearRenderTargetView(rtv, [1.0f32, 0.2, 0.4, 0.5].as_ptr(), 0, null_mut());
            self.list.IASetPrimitiveTopology(
                D3D_PRIMITIVE_TOPOLOGY::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
            );
            self.list.IASetIndexBuffer(&self.indices_buffer_view);
            self.list.IASetVertexBuffers(0, 1, &self.vertex_buffer_view);
            self.list.DrawIndexedInstanced(12, 1, 0, 0, 0);

            // Set render target to be presentable
            self.list.ResourceBarrier(
                1,
                &cd3dx12_resource_barrier_transition(
                    current_back_buffer,
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_RENDER_TARGET,
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_PRESENT,
                    None,
                    None,
                ),
            );

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
                let win = Window::new(hwnd).unwrap();
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
        let cursor = LoadCursorW(HINSTANCE(0), IDC_ARROW);
        let cls = WNDCLASSA {
            style: WNDCLASS_STYLES::CS_HREDRAW | WNDCLASS_STYLES::CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: instance,
            lpszClassName: PSTR(b"CompositionCls\0".as_ptr() as _),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hIcon: HICON(0),
            hCursor: cursor,
            hbrBackground: HBRUSH(0),
            lpszMenuName: PSTR(null_mut()),
        };
        RegisterClassA(&cls);
        let hwnd = CreateWindowExA(
            WINDOW_EX_STYLE::WS_EX_NOREDIRECTIONBITMAP as _,
            PSTR(b"CompositionCls\0".as_ptr() as _),
            PSTR(b"Composition example\0".as_ptr() as _),
            WINDOW_STYLE::WS_OVERLAPPEDWINDOW | WINDOW_STYLE::WS_VISIBLE,
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
