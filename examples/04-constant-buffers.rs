use bindings::{
    windows::win32::direct3d11::*, windows::win32::direct3d12::*, windows::win32::direct3d_hlsl::*,
    windows::win32::direct_composition::*, windows::win32::display_devices::*,
    windows::win32::dxgi::*, windows::win32::gdi::*, windows::win32::menus_and_resources::*,
    windows::win32::system_services::*, windows::win32::windows_and_messaging::*,
};
use directx_math::*;
use dx12_common::{
    cd3dx12_blend_desc_default, cd3dx12_depth_stencil_desc_default,
    cd3dx12_heap_properties_with_type, cd3dx12_rasterizer_desc_default,
    cd3dx12_resource_barrier_transition, cd3dx12_resource_desc_buffer, create_default_buffer,
};
use std::ptr::{null, null_mut};
use std::{convert::TryInto, ffi::CString};
use windows::{Abi, Interface};

const NUM_OF_FRAMES: usize = 2;

#[repr(C)]
struct ConstantBuffer {
    rotation: XMFLOAT4X4,
}

#[derive(Debug)]
#[repr(C)]
struct Vertex {
    position: XMFLOAT3,
    color: XMFLOAT4,
}
impl Vertex {
    fn new(position: [f32; 3], color: [f32; 4]) -> Self {
        Self {
            position: position.into(),
            color: color.into(),
        }
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

    cb_descriptors: [ID3D12DescriptorHeap; NUM_OF_FRAMES],
    constant_buffers: [(ID3D12Resource, *mut ConstantBuffer); NUM_OF_FRAMES],
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
                stereo: BOOL(0),
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
                r#type: D3D12_DESCRIPTOR_HEAP_TYPE::D3D12_DESCRIPTOR_HEAP_TYPE_DSV,
                num_descriptors: 1,
                flags: D3D12_DESCRIPTOR_HEAP_FLAGS::D3D12_DESCRIPTOR_HEAP_FLAG_NONE,
                node_mask: 0,
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
                        alignment: 0,
                        width: 1024,
                        height: 1024,

                        // If DXGI_SWAP_CHAIN_DESC1::Stereo is TRUE (3d glasses
                        // support) following array size needs to be 2:
                        depth_or_array_size: 1,

                        mip_levels: 1,
                        dimension: D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_TEXTURE2D,
                        sample_desc: DXGI_SAMPLE_DESC {
                            count: 1,
                            quality: 0,
                        },
                        format: DXGI_FORMAT::DXGI_FORMAT_D32_FLOAT,
                        flags: D3D12_RESOURCE_FLAGS::D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,
                        ..std::mem::zeroed()
                    },
                    // D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_COMMON,
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_DEPTH_WRITE,
                    &D3D12_CLEAR_VALUE {
                        format: DXGI_FORMAT::DXGI_FORMAT_D32_FLOAT,
                        anonymous: D3D12_CLEAR_VALUE_0 {
                            depth_stencil: D3D12_DEPTH_STENCIL_VALUE {
                                depth: 1.0,
                                stencil: 0,
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

        // Creation of constant buffer begins here -----------------------------
        //
        // Steps are roughly:
        //
        // 1. Create a heap
        // 2. Create a constant buffer resource as upload buffer, send your initial value there
        // 3. Assign your constant buffers to the root_signature

        // Create constant buffer heap
        let cb_descriptors: [ID3D12DescriptorHeap; NUM_OF_FRAMES] = (0..NUM_OF_FRAMES)
            .map(|_| unsafe {
                let mut ptr: Option<ID3D12DescriptorHeap> = None;
                device
                .CreateDescriptorHeap(
                    &D3D12_DESCRIPTOR_HEAP_DESC {
                        r#type: D3D12_DESCRIPTOR_HEAP_TYPE::D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                        num_descriptors: 1,
                        flags:
                            D3D12_DESCRIPTOR_HEAP_FLAGS::D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                        node_mask: 0,
                    },
                    &ID3D12DescriptorHeap::IID,
                    ptr.set_abi(),
                )
                .and_some(ptr)
                .unwrap()
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

        // Create constant buffer resources
        let constant_buffers: [(ID3D12Resource, *mut ConstantBuffer); NUM_OF_FRAMES] = (0
            ..NUM_OF_FRAMES)
            .map(|i| unsafe {
                // Constant buffers must be sized in 256 byte chunks
                let value_size = std::mem::size_of::<ConstantBuffer>();
                let cb_size_in_bytes = (value_size + 255) & !255;

                // Generic way to create upload buffer and get address:
                let mut ptr: Option<ID3D12Resource> = None;
                let cb = device
                    .CreateCommittedResource(
                        &cd3dx12_heap_properties_with_type(D3D12_HEAP_TYPE::D3D12_HEAP_TYPE_UPLOAD),
                        D3D12_HEAP_FLAGS::D3D12_HEAP_FLAG_NONE,
                        &cd3dx12_resource_desc_buffer(cb_size_in_bytes as _, None, None),
                        D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_GENERIC_READ,
                        null(),
                        &ID3D12Resource::IID,
                        ptr.set_abi(),
                    )
                    .and_some(ptr)
                    .expect("Unable to create constant buffer resource");

                let mut cb_memory_ptr = null_mut::<ConstantBuffer>();
                cb.Map(
                    0,
                    &D3D12_RANGE { begin: 0, end: 0 },
                    &mut cb_memory_ptr as *mut *mut _ as *mut *mut _,
                )
                .ok()
                .expect("Unable to get memory location for constant buffer");

                // Store 45 degree rotation to matrix
                let mat = XMMatrixMultiply(XMMatrixIdentity(), &XMMatrixRotationZ(XM_PI / 4.0));
                XMStoreFloat4x4(&mut (*cb_memory_ptr).rotation, mat);

                // Assign the upload buffer as constant buffer view
                let offset = cb.GetGPUVirtualAddress();
                device.CreateConstantBufferView(
                    &D3D12_CONSTANT_BUFFER_VIEW_DESC {
                        buffer_location: offset,
                        size_in_bytes: cb_size_in_bytes as _,
                    },
                    cb_descriptors[i].GetCPUDescriptorHandleForHeapStart(),
                );

                (cb, cb_memory_ptr)
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();

        // Create root signature
        let root_signature = unsafe {
            let root = {
                let mut blob: Option<ID3DBlob> = None;
                let mut error: Option<ID3DBlob> = None;

                let mut params = D3D12_ROOT_PARAMETER {
                    parameter_type: D3D12_ROOT_PARAMETER_TYPE::D3D12_ROOT_PARAMETER_TYPE_CBV,
                    anonymous: D3D12_ROOT_PARAMETER_0 {
                        descriptor: D3D12_ROOT_DESCRIPTOR {
                            register_space: 0,
                            shader_register: 0,
                        },
                    },
                    shader_visibility: D3D12_SHADER_VISIBILITY::D3D12_SHADER_VISIBILITY_VERTEX,
                };

                let desc = D3D12_ROOT_SIGNATURE_DESC {
                    num_parameters: 1,
                    p_parameters: &mut params,
                    num_static_samplers: 0,
                    p_static_samplers: null_mut() as _,
                    flags: D3D12_ROOT_SIGNATURE_FLAGS::from(
                        D3D12_ROOT_SIGNATURE_FLAGS::D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT.0 |
                        D3D12_ROOT_SIGNATURE_FLAGS::D3D12_ROOT_SIGNATURE_FLAG_DENY_HULL_SHADER_ROOT_ACCESS.0 |
                        D3D12_ROOT_SIGNATURE_FLAGS::D3D12_ROOT_SIGNATURE_FLAG_DENY_GEOMETRY_SHADER_ROOT_ACCESS.0 |
                        D3D12_ROOT_SIGNATURE_FLAGS::D3D12_ROOT_SIGNATURE_FLAG_DENY_PIXEL_SHADER_ROOT_ACCESS.0
                    ),
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

        // End of constant buffer changes ----------------------------------

        let vertex_shader = unsafe {
            let data = include_bytes!("./04-constant-buffers.hlsl");
            let mut err: Option<ID3DBlob> = None;
            let mut ptr: Option<ID3DBlob> = None;

            D3DCompile(
                data.as_ptr() as *mut _,
                data.len(),
                PSTR("shaders.hlsl\0".as_ptr() as _),
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
            let data = include_bytes!("./04-constant-buffers.hlsl");
            let mut err: Option<ID3DBlob> = None;
            let mut ptr: Option<ID3DBlob> = None;

            D3DCompile(
                data.as_ptr() as *mut _,
                data.len(),
                PSTR("shaders.hlsl\0".as_ptr() as _),
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
            rasterizer_state: cd3dx12_rasterizer_desc_default(),
            blend_state: cd3dx12_blend_desc_default(),
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
            dsv_format: DXGI_FORMAT::DXGI_FORMAT_D32_FLOAT,
            depth_stencil_state: cd3dx12_depth_stencil_desc_default(),
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
                buffer_location: vertex_buffers.gpu_buffer.GetGPUVirtualAddress(),
                stride_in_bytes: std::mem::size_of::<Vertex>() as _,
                size_in_bytes: vertices_as_bytes.len() as _,
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
                buffer_location: buffers.gpu_buffer.GetGPUVirtualAddress(),
                size_in_bytes: indicies_as_bytes.len() as _,
                format: DXGI_FORMAT::DXGI_FORMAT_R32_UINT,
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
            cb_descriptors,
            constant_buffers,
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
            self.list.SetGraphicsRootConstantBufferView(
                0,
                self.constant_buffers[self.current_frame]
                    .0
                    .GetGPUVirtualAddress(),
            );
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
        let cursor = LoadCursorA(HINSTANCE(0), PSTR(IDC_ARROW as _));
        let cls = WNDCLASSA {
            style: WNDCLASS_STYLES::CS_HREDRAW | WNDCLASS_STYLES::CS_VREDRAW,
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
            PSTR(b"Constant Buffer example\0".as_ptr() as _),
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
