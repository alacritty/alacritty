
use CreationError;
use GlAttributes;
use GlProfile;
use GlRequest;
use PixelFormatRequirements;
use ReleaseBehavior;
use cocoa::appkit::*;
use cocoa::base::nil;

pub fn get_gl_profile<T>(
    opengl: &GlAttributes<&T>,
    pf_reqs: &PixelFormatRequirements,
) -> Result<NSOpenGLPFAOpenGLProfiles, CreationError> {
    let version = opengl.version.to_gl_version();
    // first, compatibility profile support is strict
    if opengl.profile == Some(GlProfile::Compatibility) {
        // Note: we are not using ranges because of a rust bug that should be fixed here:
        // https://github.com/rust-lang/rust/pull/27050
        if version.unwrap_or((2, 1)) < (3, 2) {
            Ok(NSOpenGLProfileVersionLegacy)
        } else {
            Err(CreationError::OpenGlVersionNotSupported)
        }
    } else if let Some(v) = version {
        // second, process exact requested version, if any
        if v < (3, 2) {
            if opengl.profile.is_none() && v <= (2, 1) {
                Ok(NSOpenGLProfileVersionLegacy)
            } else {
                Err(CreationError::OpenGlVersionNotSupported)
            }
        } else if v == (3, 2) {
            Ok(NSOpenGLProfileVersion3_2Core)
        } else {
            Ok(NSOpenGLProfileVersion4_1Core)
        }
    } else if let GlRequest::Latest = opengl.version {
        // now, find the latest supported version automatically;
        let mut attributes: [u32; 6] = [0; 6];
        let mut current_idx = 0;
        attributes[current_idx] = NSOpenGLPFAAllowOfflineRenderers as u32;
        current_idx += 1;
        
        if let Some(true) = pf_reqs.hardware_accelerated {
            attributes[current_idx] = NSOpenGLPFAAccelerated as u32;
            current_idx += 1;
        }

        if pf_reqs.double_buffer != Some(false) {
            attributes[current_idx] = NSOpenGLPFADoubleBuffer as u32;
            current_idx += 1
        }
        
        attributes[current_idx] = NSOpenGLPFAOpenGLProfile as u32;
        current_idx += 1;
            
        for &profile in &[NSOpenGLProfileVersion4_1Core, NSOpenGLProfileVersion3_2Core] {
            attributes[current_idx] = profile as u32;
            let id = unsafe {
                NSOpenGLPixelFormat::alloc(nil).initWithAttributes_(&attributes)
            };
            if id != nil {
                unsafe { msg_send![id, release] }
                return Ok(profile);
            }
        }
        // nothing else to do
        Ok(NSOpenGLProfileVersionLegacy)
    } else {
        Err(CreationError::OpenGlVersionNotSupported)
    }
}

pub fn build_nsattributes(
    pf_reqs: &PixelFormatRequirements, profile: NSOpenGLPFAOpenGLProfiles
) -> Result<Vec<u32>, CreationError> {
    // NOTE: OS X no longer has the concept of setting individual
    // color component's bit size. Instead we can only specify the
    // full color size and hope for the best. Another hiccup is that
    // `NSOpenGLPFAColorSize` also includes `NSOpenGLPFAAlphaSize`,
    // so we have to account for that as well.
    let alpha_depth = pf_reqs.alpha_bits.unwrap_or(8);
    let color_depth = pf_reqs.color_bits.unwrap_or(24) + alpha_depth;

    let mut attributes = vec![
        NSOpenGLPFAOpenGLProfile as u32, profile as u32,
        NSOpenGLPFAClosestPolicy as u32,
        NSOpenGLPFAColorSize as u32, color_depth as u32,
        NSOpenGLPFAAlphaSize as u32, alpha_depth as u32,
        NSOpenGLPFADepthSize as u32, pf_reqs.depth_bits.unwrap_or(24) as u32,
        NSOpenGLPFAStencilSize as u32, pf_reqs.stencil_bits.unwrap_or(8) as u32,
        NSOpenGLPFAAllowOfflineRenderers as u32,
    ];

    if let Some(true) = pf_reqs.hardware_accelerated {
        attributes.push(NSOpenGLPFAAccelerated as u32);
    }

    // Note: according to Apple docs, not specifying `NSOpenGLPFADoubleBuffer`
    // equals to requesting a single front buffer, in which case most of the GL
    // renderers will show nothing, since they draw to GL_BACK. 
    if pf_reqs.double_buffer != Some(false) {
        attributes.push(NSOpenGLPFADoubleBuffer as u32);
    }

    if pf_reqs.release_behavior != ReleaseBehavior::Flush {
        return Err(CreationError::NoAvailablePixelFormat);
    }

    if pf_reqs.stereoscopy {
        unimplemented!();   // TODO:
    }

    if pf_reqs.float_color_buffer {
        attributes.push(NSOpenGLPFAColorFloat as u32);
    }

    if let Some(samples) = pf_reqs.multisampling {
        attributes.push(NSOpenGLPFAMultisample as u32);
        attributes.push(NSOpenGLPFASampleBuffers as u32); attributes.push(1);
        attributes.push(NSOpenGLPFASamples as u32); attributes.push(samples as u32);
    }

    // attribute list must be null terminated.
    attributes.push(0);

    Ok(attributes)
}
