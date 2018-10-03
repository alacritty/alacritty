# Sharing Readme

Context sharing is an advanced topic and this readme hopes to gather a small
subsection of the complex rules relating to it. I recommend that you take some
time after reading this document to read the docs/specs for your target
platform(s), as they can impose additional restrictions. Extensions and your
chosen client api can also impose additional restrictions (yeah, good luck
reading every OpenGL extension out there). 

Like usual, the best way to check that your program works on your target
platform(s) is of course to test it on them :)

# EGL

EGL is used sometimes on windows and X11. It is always used when running on
Wayland and Android.

From: https://www.khronos.org/registry/EGL/specs/eglspec.1.5.pdf

§3.7.1:

> The OpenGL and OpenGL ES server context state for all sharing contexts must
> exist in a single address space.

§2.4:

> Such state may only be shared between different contexts of the same API type
> (e.g. two OpenGL contexts, two OpenGL ES contexts, or two OpenVG contexts, but
> not a mixture).

> EGL provides for sharing certain types of context state among contexts
> existing in a single address space. The types of client API objects that are
> shareable are defined by the corresponding client API specifications.

§2.3:

> In a multi-threaded environment, all threads may share the same virtual
> address space; however, this capability is not required, and implementations
> may choose to restrict their address space to be per-thread even in an
> environment supporting multiple application threads.

(Yeah, sharing contexts made on different threads can be a violation.)

> Context state, including both the client and server state of OpenGL and OpenGL
> ES contexts, exists in the client’s address space; this state cannot be shared
> by a client in another process.

> Support of indirect rendering (in those environments where this concept makes
> sense) may have the effect of relaxing these limits on sharing.  However, such
> support is beyond the scope of this document.

§3.7.1.6

> An EGL_BAD_CONTEXT error is generated if share context is neither
> EGL_NO_CONTEXT nor a valid context of the same client API type as the newly
> created context.
    
> An EGL_BAD_MATCH error is generated if an OpenGL or OpenGL ES context is
> requested and any of:
> 
>     • the server context state for share context exists in an address space
>     that cannot be shared with the newly created context 
> 
>     • share context was created on a different display than the one
>     referenced by config 
>    
>     • the reset notification behavior of share context and the newly
>     created context are different 
> 
>     • the contexts are otherwise incompatible (for example, one context
>     being associated with a hardware device driver and the other with a
>     software renderer).

# GLX

GLX is sometimes used on X11.

From: https://www.khronos.org/registry/OpenGL/specs/gl/glx1.3.pdf

§2.4:

> GLX provides for limited sharing of display lists. Since the lists are part of
> the server context state they can be shared only if the server state for the
> sharing contexts exists in a single address space.

§3.3.7:

> The server context state for all sharing contexts must exist in a single
> address space or a BadMatch error is generated.

> glXCreateNewContext **can** generate the following errors: GLXBadContext if
> share list is neither zero nor a valid GLX rendering context;

(Emphasis mine)

# WGL

WGL is sometimes used on windows.

From:
https://www.khronos.org/registry/OpenGL/extensions/ARB/WGL_ARB_create_context.txt

Under "Additions to the WGL specification":

>  • If <hShareContext> is neither zero nor a valid context handle, then
>  ERROR_INVALID_OPERATION is generated.


>  • If the OpenGL server context state for <hShareContext> exists in an address
>  space that cannot be shared with the newly created context, if <shareContext>
>  was created on a different device context than the one referenced by <hDC>,
>  or if the contexts are otherwise incompatible (for example, one context being
>  associated with a hardware device driver and the other with a software
>  renderer), then ERROR_INVALID_OPERATION is generated.

Under "Issues":

> 1. Can different GL context versions share data?
> 
> PROPOSED: Yes, with restrictions as defined by the supported feature sets.
> For example, program and shader objects cannot be shared with OpenGL 1.x
> contexts, which do not support them.
> 
> NOTE: When the new object model is introduced, sharing must be established at
> creation time, since the object handle namespace is also shared.
> wglShareLists would therefore fail if either context parameter to it were to
> be a context supporting the new object model.

> 15. How is context sharing between contexts of different versions handled?
> 
> RESOLVED: It's up to the implementation whether or not to allow this, and to
> define behavior when shared objects include state or behaviors not described
> by one of the contexts sharing them (for example, textures with nonzero width
> borders shared by 3.2 core and compatibility profile contexts).

(And I'm guessing it's UB if you do something your implementation doesn't allow)
