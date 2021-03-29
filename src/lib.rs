#![allow(unused_imports)]
/// CD3DX12 Helper functions from here:
/// https://github.com/microsoft/DirectX-Graphics-Samples/blob/master/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h
use bindings::{
    windows::win32::direct3d11::*, windows::win32::direct3d12::*, windows::win32::direct3d_hlsl::*,
    windows::win32::direct_composition::*, windows::win32::display_devices::*,
    windows::win32::dxgi::*, windows::win32::gdi::*, windows::win32::menus_and_resources::*,
    windows::win32::system_services::*, windows::win32::windows_and_messaging::*,
};
use directx_math::*;
use std::{convert::TryInto, ffi::CString, mem};
use std::{ffi::c_void, ptr::null_mut};
use windows::{Abi, Interface};

pub struct Buffers {
    pub upload_buffer: ID3D12Resource,
    pub gpu_buffer: ID3D12Resource,
}

/// Creates a gpu buffer from given data
///
/// Returns also upload buffer that must be kept alive until the command list is
/// executed.
pub fn create_default_buffer(
    device: &ID3D12Device,
    list: &ID3D12GraphicsCommandList,
    data: &[u8],
) -> ::windows::Result<Buffers> {
    let default_buffer = unsafe {
        let mut ptr: Option<ID3D12Resource> = None;
        device
            .CreateCommittedResource(
                &cd3dx12_heap_properties_with_type(D3D12_HEAP_TYPE::D3D12_HEAP_TYPE_DEFAULT),
                D3D12_HEAP_FLAGS::D3D12_HEAP_FLAG_NONE,
                &cd3dx12_resource_desc_buffer(data.len() as _, None, None),
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_COMMON,
                null_mut(),
                &ID3D12Resource::IID,
                ptr.set_abi(),
            )
            .and_some(ptr)
    }?;

    let upload_buffer = unsafe {
        let mut ptr: Option<ID3D12Resource> = None;
        device
            .CreateCommittedResource(
                &cd3dx12_heap_properties_with_type(D3D12_HEAP_TYPE::D3D12_HEAP_TYPE_UPLOAD),
                D3D12_HEAP_FLAGS::D3D12_HEAP_FLAG_NONE,
                &cd3dx12_resource_desc_buffer(data.len() as _, None, None),
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_GENERIC_READ,
                null_mut(),
                &ID3D12Resource::IID,
                ptr.set_abi(),
            )
            .and_some(ptr)
    }?;

    let mut sub_data = D3D12_SUBRESOURCE_DATA {
        p_data: data.as_ptr() as *mut _,
        row_pitch: data.len() as _,
        slice_pitch: data.len() as _,
        ..Default::default()
    };

    unsafe {
        list.ResourceBarrier(
            1,
            &cd3dx12_resource_barrier_transition(
                &default_buffer,
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_COMMON,
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_COPY_DEST,
                None,
                None,
            ),
        );
    }

    update_subresources(
        &list,
        &default_buffer,
        &upload_buffer,
        0,
        0,
        1,
        &mut sub_data,
        1,
    )?;

    unsafe {
        list.ResourceBarrier(
            1,
            &cd3dx12_resource_barrier_transition(
                &default_buffer,
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_COPY_DEST,
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_GENERIC_READ,
                None,
                None,
            ),
        );
    }
    Ok(Buffers {
        gpu_buffer: default_buffer,
        upload_buffer,
    })
}

// #[derive(Debug)]
// pub struct ConstantBuffer<T: Sized> {
//     upload_buffer: UploadBuffer<T>,
//     shader_visibility: D3D12_SHADER_VISIBILITY,
// }

#[derive(Debug)]
pub struct UploadBuffer<T: Sized> {
    buffer: ID3D12Resource,
    aligned_size: usize,
    gpu_memory_ptr: *mut T,
}

impl<T: Sized> UploadBuffer<T> {
    pub fn new(device: &ID3D12Device, init_data: &T) -> ::windows::Result<UploadBuffer<T>> {
        unsafe {
            let value_size = std::mem::size_of::<T>();
            let aligned_size = (value_size + 255) & !255;

            // Generic way to create upload buffer and get address:
            let mut ptr: Option<ID3D12Resource> = None;
            let buffer = device
                .CreateCommittedResource(
                    &cd3dx12_heap_properties_with_type(D3D12_HEAP_TYPE::D3D12_HEAP_TYPE_UPLOAD),
                    D3D12_HEAP_FLAGS::D3D12_HEAP_FLAG_NONE,
                    &cd3dx12_resource_desc_buffer(aligned_size as _, None, None),
                    D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_GENERIC_READ,
                    std::ptr::null(),
                    &ID3D12Resource::IID,
                    ptr.set_abi(),
                )
                .and_some(ptr)
                .expect("Unable to create constant buffer resource");

            // Notice that the memory location is left mapped
            let mut gpu_memory_ptr = null_mut::<T>();
            buffer
                .Map(
                    0,
                    &D3D12_RANGE { begin: 0, end: 0 },
                    &mut gpu_memory_ptr as *mut *mut _ as *mut *mut _,
                )
                .ok()
                .expect("Unable to get memory location for constant buffer");

            std::ptr::copy_nonoverlapping(init_data, gpu_memory_ptr, 1);

            Ok(UploadBuffer {
                aligned_size,
                buffer,
                gpu_memory_ptr,
            })
        }
    }

    pub fn update(&mut self, value: &T) {
        unsafe {
            std::ptr::copy_nonoverlapping(value, self.gpu_memory_ptr, 1);
        }
    }

    pub fn gpu_virtual_address(&self) -> u64 {
        unsafe { self.buffer.GetGPUVirtualAddress() }
    }

    pub fn create_constant_buffer_view(
        &self,
        device: &ID3D12Device,
        cbv_heap: &ID3D12DescriptorHeap,
    ) {
        // TODO: Should I instead create and output ID3D12DescriptorHeap?
        unsafe {
            device.CreateConstantBufferView(
                &D3D12_CONSTANT_BUFFER_VIEW_DESC {
                    buffer_location: self.gpu_virtual_address(),
                    size_in_bytes: self.aligned_size as _,
                },
                cbv_heap.GetCPUDescriptorHandleForHeapStart(),
            );
        }
    }
}

impl<T> Drop for UploadBuffer<T> {
    fn drop(&mut self) {
        unsafe {
            self.buffer.Unmap(0, std::ptr::null());
        }
    }
}

pub fn create_upload_buffer(
    device: &ID3D12Device,
    data: &[u8],
) -> ::windows::Result<ID3D12Resource> {
    unsafe {
        let props = D3D12_HEAP_PROPERTIES {
            r#type: D3D12_HEAP_TYPE::D3D12_HEAP_TYPE_UPLOAD,
            cpu_page_property: D3D12_CPU_PAGE_PROPERTY::D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
            creation_node_mask: 1,
            visible_node_mask: 1,
            memory_pool_preference: D3D12_MEMORY_POOL::D3D12_MEMORY_POOL_UNKNOWN,
        };
        let desc = D3D12_RESOURCE_DESC {
            alignment: 0,
            flags: D3D12_RESOURCE_FLAGS::D3D12_RESOURCE_FLAG_NONE,
            dimension: D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_BUFFER,
            depth_or_array_size: 1,
            format: DXGI_FORMAT::DXGI_FORMAT_UNKNOWN,
            height: 1,
            width: data.len() as u64,
            layout: D3D12_TEXTURE_LAYOUT::D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            mip_levels: 1,
            sample_desc: DXGI_SAMPLE_DESC {
                count: 1,
                quality: 0,
            },
        };
        let mut ptr: Option<ID3D12Resource> = None;
        let resource = device
            .CreateCommittedResource(
                &props,
                D3D12_HEAP_FLAGS::D3D12_HEAP_FLAG_NONE,
                &desc,
                D3D12_RESOURCE_STATES::D3D12_RESOURCE_STATE_GENERIC_READ,
                null_mut(),
                &ID3D12Resource::IID,
                ptr.set_abi(),
            )
            .and_some(ptr)?;

        let mut gpu_data: *mut u8 = null_mut();
        resource
            .Map(
                0,
                &D3D12_RANGE { begin: 0, end: 0 },
                &mut gpu_data as *mut *mut _ as *mut *mut _,
            )
            .ok()?;

        if gpu_data.is_null() {
            panic!("Failed to map");
        }
        std::ptr::copy_nonoverlapping(data.as_ptr(), gpu_data, data.len());

        // Debug, if you want to see what was copied
        // let gpu_slice = std::slice::from_raw_parts(gpu_triangle, 3);
        // println!("{:?}", cpu_triangle);
        // println!("{:?}", gpu_slice);

        resource.Unmap(0, null_mut());
        Ok(resource)
    }
}

pub fn cd3dx12_heap_properties_with_type(heap_type: D3D12_HEAP_TYPE) -> D3D12_HEAP_PROPERTIES {
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L423-L433
    D3D12_HEAP_PROPERTIES {
        r#type: heap_type,
        cpu_page_property: D3D12_CPU_PAGE_PROPERTY::D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        memory_pool_preference: D3D12_MEMORY_POOL::D3D12_MEMORY_POOL_UNKNOWN,
        creation_node_mask: 1,
        visible_node_mask: 1,
    }
}

pub const fn cd3dx12_depth_stencil_desc_default() -> D3D12_DEPTH_STENCIL_DESC {
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L177-L189
    D3D12_DEPTH_STENCIL_DESC {
        depth_enable: BOOL(1),
        depth_write_mask: D3D12_DEPTH_WRITE_MASK::D3D12_DEPTH_WRITE_MASK_ALL,
        depth_func: D3D12_COMPARISON_FUNC::D3D12_COMPARISON_FUNC_LESS,
        stencil_enable: BOOL(0),
        stencil_read_mask: D3D12_DEFAULT_STENCIL_READ_MASK as _,
        stencil_write_mask: D3D12_DEFAULT_STENCIL_WRITE_MASK as _,
        front_face: D3D12_DEPTH_STENCILOP_DESC {
            stencil_depth_fail_op: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            stencil_fail_op: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            stencil_pass_op: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            stencil_func: D3D12_COMPARISON_FUNC::D3D12_COMPARISON_FUNC_ALWAYS,
        },
        back_face: D3D12_DEPTH_STENCILOP_DESC {
            stencil_depth_fail_op: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            stencil_fail_op: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            stencil_pass_op: D3D12_STENCIL_OP::D3D12_STENCIL_OP_KEEP,
            stencil_func: D3D12_COMPARISON_FUNC::D3D12_COMPARISON_FUNC_ALWAYS,
        },
    }
}

pub fn cd3dx12_blend_desc_default() -> D3D12_BLEND_DESC {
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L323-L338
    D3D12_BLEND_DESC {
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
                render_target_write_mask: D3D12_COLOR_WRITE_ENABLE::D3D12_COLOR_WRITE_ENABLE_ALL.0
                    as _,
            })
            .collect::<Vec<_>>()
            .as_slice()
            .try_into()
            .unwrap(),
    }
}

pub fn cd3dx12_rasterizer_desc_default() -> D3D12_RASTERIZER_DESC {
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L349-L359
    D3D12_RASTERIZER_DESC {
        fill_mode: D3D12_FILL_MODE::D3D12_FILL_MODE_SOLID,
        cull_mode: D3D12_CULL_MODE::D3D12_CULL_MODE_BACK,
        front_counter_clockwise: false.into(),
        depth_bias: D3D12_DEFAULT_DEPTH_BIAS as _,
        depth_bias_clamp: D3D12_DEFAULT_DEPTH_BIAS_CLAMP,
        slope_scaled_depth_bias: D3D12_DEFAULT_SLOPE_SCALED_DEPTH_BIAS,
        depth_clip_enable: true.into(),
        multisample_enable: false.into(),
        antialiased_line_enable: false.into(),
        forced_sample_count: 0,
        conservative_raster:
            D3D12_CONSERVATIVE_RASTERIZATION_MODE::D3D12_CONSERVATIVE_RASTERIZATION_MODE_OFF,
    }
}

pub fn cd3dx12_resource_desc_buffer(
    width: u64,
    flags: Option<D3D12_RESOURCE_FLAGS>,
    alignment: Option<u64>,
) -> D3D12_RESOURCE_DESC {
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/master/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L1754-L1756
    // Order follows the C++ function call order
    D3D12_RESOURCE_DESC {
        dimension: D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_BUFFER,
        alignment: alignment.unwrap_or(0),
        width,
        depth_or_array_size: 1,
        height: 1,
        mip_levels: 1,
        format: DXGI_FORMAT::DXGI_FORMAT_UNKNOWN,
        sample_desc: DXGI_SAMPLE_DESC {
            count: 1,
            quality: 0,
        },
        layout: D3D12_TEXTURE_LAYOUT::D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        flags: flags.unwrap_or(D3D12_RESOURCE_FLAGS::D3D12_RESOURCE_FLAG_NONE),
    }
}

pub fn cd3dx12_resource_desc_tex2d(
    format: DXGI_FORMAT,
    width: u64,
    height: u32,
    array_size: Option<u16>,
    mip_levels: Option<u16>,
    sample_count: Option<u32>,
    sample_quality: Option<u32>,
    flags: Option<D3D12_RESOURCE_FLAGS>,
    layout: Option<D3D12_TEXTURE_LAYOUT>,
    alignment: Option<u64>,
) -> D3D12_RESOURCE_DESC {
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L1773-L1787
    D3D12_RESOURCE_DESC {
        dimension: D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        alignment: alignment.unwrap_or(0),
        width,
        depth_or_array_size: array_size.unwrap_or(1),
        height,
        mip_levels: mip_levels.unwrap_or(0),
        format,
        sample_desc: DXGI_SAMPLE_DESC {
            count: sample_count.unwrap_or(1),
            quality: sample_quality.unwrap_or(0),
        },
        layout: layout.unwrap_or(D3D12_TEXTURE_LAYOUT::D3D12_TEXTURE_LAYOUT_UNKNOWN),
        flags: flags.unwrap_or(D3D12_RESOURCE_FLAGS::D3D12_RESOURCE_FLAG_NONE),
    }
}

pub fn cd3dx12_resource_barrier_transition(
    resource: &ID3D12Resource,
    state_before: D3D12_RESOURCE_STATES,
    state_after: D3D12_RESOURCE_STATES,
    subresource: Option<u32>,
    flags: Option<D3D12_RESOURCE_BARRIER_FLAGS>,
) -> D3D12_RESOURCE_BARRIER {
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L728-L744
    let subresource = subresource.unwrap_or(D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES);
    let flags = flags.unwrap_or(D3D12_RESOURCE_BARRIER_FLAGS::D3D12_RESOURCE_BARRIER_FLAG_NONE);

    let mut barrier = D3D12_RESOURCE_BARRIER {
        r#type: D3D12_RESOURCE_BARRIER_TYPE::D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,
        flags,
        ..unsafe { std::mem::zeroed() }
    };
    barrier.anonymous.transition.subresource = subresource;
    barrier.anonymous.transition.p_resource = resource.abi();
    barrier.anonymous.transition.state_before = state_before;
    barrier.anonymous.transition.state_after = state_after;
    barrier
}

pub fn cd3dx12_texture_copy_location_sub(
    res: &ID3D12Resource,
    sub: u32,
) -> D3D12_TEXTURE_COPY_LOCATION {
    let mut res = D3D12_TEXTURE_COPY_LOCATION {
        // TODO: This should be pointer, can I get rid of clone?
        p_resource: Some(res.clone()),
        r#type: D3D12_TEXTURE_COPY_TYPE::D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
        ..unsafe { std::mem::zeroed() }
    };

    res.anonymous.placed_footprint = D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
        ..unsafe { std::mem::zeroed() }
    };
    res.anonymous.subresource_index = sub;
    res
}

pub fn cd3dx12_texture_copy_location_footprint(
    res: &ID3D12Resource,
    footprint: &D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
) -> D3D12_TEXTURE_COPY_LOCATION {
    let mut res = D3D12_TEXTURE_COPY_LOCATION {
        // TODO: This should be pointer, can I get rid of clone?
        p_resource: Some(res.clone()),
        r#type: D3D12_TEXTURE_COPY_TYPE::D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
        ..unsafe { std::mem::zeroed() }
    };
    res.anonymous.placed_footprint = footprint.clone();
    res
}

/// WinAPI equivalent of SIZE_T(-1)
///
/// This is also bitwise not zero !0 or (in C++ ~0), not sure why the hell it's
/// written as SIZE_T(-1)
const SIZE_T_MINUS1: usize = usize::MAX;

/// Update subresources
//
/// This is mimicking stack allocation implementation, but since Rust doesn't
/// have const generics, I think only way is to allocate in heap.
pub fn update_subresources(
    list: &ID3D12GraphicsCommandList,
    dest_resource: &ID3D12Resource,
    intermediate: &ID3D12Resource,
    intermediate_offset: u64,
    first_subresource: u32,
    num_subresources: u32,
    p_src_data: *mut D3D12_SUBRESOURCE_DATA,
    max_subresources: usize,
) -> ::windows::Result<u64> {
    // Stack alloc implementation but with vecs
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L2118-L2140
    let src_data = unsafe { std::slice::from_raw_parts_mut(p_src_data, num_subresources as _) };
    let mut required_size = 0;
    let mut layouts_vec = vec![D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default(); max_subresources];
    let layouts = layouts_vec.as_mut_slice();
    let mut num_rows_vec = vec![0; max_subresources];
    let num_rows = num_rows_vec.as_mut_slice();
    let mut row_sizes_in_bytes_vec = vec![0; max_subresources];
    let row_sizes_in_bytes = row_sizes_in_bytes_vec.as_mut_slice();
    let desc = unsafe { dest_resource.GetDesc() };
    unsafe {
        let dest_device = {
            let mut ptr: Option<ID3D12Device> = None;
            dest_resource
                .GetDevice(&ID3D12Device::IID, ptr.set_abi())
                .and_some(ptr)
        }?;
        dest_device.GetCopyableFootprints(
            &desc,
            first_subresource,
            num_subresources as _,
            intermediate_offset,
            layouts.as_mut_ptr(),
            num_rows.as_mut_ptr(),
            row_sizes_in_bytes.as_mut_ptr(),
            &mut required_size,
        );
    }

    // UpdateSubresources main implementation
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L2036-L2076

    // Minor validation
    let intermediate_desc = unsafe { intermediate.GetDesc() };
    let dest_desc = unsafe { dest_resource.GetDesc() };
    if intermediate_desc.dimension != D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_BUFFER
        || intermediate_desc.width < (required_size + layouts[0].offset)
        || required_size > (SIZE_T_MINUS1 as u64)
        || (dest_desc.dimension == D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_BUFFER
            && (first_subresource != 0 || num_subresources != 1))
    {
        return Ok(0); // TODO: Is this actually a failure?
    }

    let mut p_data = null_mut();

    unsafe { intermediate.Map(0, null_mut(), &mut p_data) }.ok()?;

    for i in 0..(num_subresources as usize) {
        if row_sizes_in_bytes[i] > (SIZE_T_MINUS1 as u64) {
            return Ok(0); // TODO: Is this actually a failure?
        }

        let mut dest_data = D3D12_MEMCPY_DEST {
            p_data: ((p_data as u64) + layouts[i].offset) as *mut _,
            row_pitch: layouts[i].footprint.row_pitch as _,
            slice_pitch: mem::size_of_val(&layouts[i].footprint.row_pitch)
                * mem::size_of_val(&num_rows[i]),
        };
        memcpy_subresource(
            &mut dest_data,
            &src_data[i],
            row_sizes_in_bytes[i] as _,
            num_rows[i],
            layouts[i].footprint.depth,
        )
    }
    unsafe {
        intermediate.Unmap(0, null_mut());
    }

    if dest_desc.dimension == D3D12_RESOURCE_DIMENSION::D3D12_RESOURCE_DIMENSION_BUFFER {
        unsafe {
            list.CopyBufferRegion(
                dest_resource,
                0,
                intermediate,
                layouts[0].offset,
                layouts[0].footprint.width as _,
            );
        }
    } else {
        // TODO: Never tested
        for i in 0..(num_subresources as usize) {
            let dst =
                cd3dx12_texture_copy_location_sub(&dest_resource, (i as u32) + first_subresource);
            let src = cd3dx12_texture_copy_location_footprint(&intermediate, &layouts[i]);
            unsafe {
                list.CopyTextureRegion(&dst, 0, 0, 0, &src, null_mut());
            }
        }
    }

    return Ok(required_size);
}

/// Row-by-row memcpy
pub fn memcpy_subresource(
    dest: *mut D3D12_MEMCPY_DEST,
    src: *const D3D12_SUBRESOURCE_DATA,
    row_size_in_bytes: usize,
    num_rows: u32,
    num_slices: u32,
) {
    // TODO: Tested only with num_rows = 1, num_slices = 1
    // unsafe {
    //     println!("dest {:?}", *dest);
    //     println!("src {:?}", *src);
    //     println!("num_rows {}", num_rows);
    //     println!("num_slices {}", num_slices);
    //     println!("row_size_in_bytes {}", row_size_in_bytes);
    // }
    // https://github.com/microsoft/DirectX-Graphics-Samples/blob/58b6bb18b928d79e5bd4e5ba53b274bdf6eb39e5/Samples/Desktop/D3D12HelloWorld/src/HelloTriangle/d3dx12.h#L1983-L2001
    for z in 0..(num_slices as usize) {
        unsafe {
            let dest_slice = ((*dest).p_data as usize) + (*dest).slice_pitch * z;
            let src_slice = ((*src).p_data as usize) + ((*src).slice_pitch as usize) * z;
            for y in 0..(num_rows as usize) {
                std::ptr::copy_nonoverlapping(
                    (src_slice + ((*src).row_pitch as usize) * y) as *const u8,
                    (dest_slice + (*dest).row_pitch * y) as *mut u8,
                    row_size_in_bytes,
                );
            }
        }
    }

    // unsafe {
    //     #[derive(Debug)]
    //     #[repr(C)]
    //     struct Vertex {
    //         position: [f32; 3],
    //         color: [f32; 4],
    //     }

    //     let src_slice_view = std::slice::from_raw_parts((*src).p_data as *const Vertex, 3);
    //     let dest_slice_view = std::slice::from_raw_parts((*dest).p_data as *const Vertex, 3);
    //     println!("{:?}", src_slice_view);
    //     println!("{:?}", dest_slice_view);
    // }
}
