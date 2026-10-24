# Caravan

Caravan is a Camel source library for authoring small 2D and 3D scene descriptions for the existing [NexusGL](https://github.com/LuminaryLabs-Dev/NexusGL) renderer. It does not introduce a graphics language or renderer. Its records mirror the data needed to build NexusGL's `scene::CanvasFrame` and `scene::SceneFrame` values.

## Current boundary

`caravan.camel` supplies reusable typed constructors and grid operations. `nexusgl_adapter.hpp` provides C++ host-side value types and converts them to NexusGL `CanvasFrame` / `SceneFrame` descriptors before calling `nexusgl::Renderer`; the host presents the returned RGBA pixels. XY grids can be submitted as 2D canvas draws. XY, XZ, and YZ grids can be submitted as 3D geometry when the host supplies a view-projection matrix, camera, and light. Camel CLI and Studio do not currently register a NexusGL renderer handler, so importing this library alone authors data but does not draw a window or image. A Camel host must decode the returned JSON into the matching `caravan` C++ value type and call the adapter.

The initial sparse grid API is `NewGrid2D`, `SetGridCell2D`, `RemoveGridCell2D`, and `GridCellCenter3D`. It stores occupied cells only; setting the same q/r replaces that cell, and removing an absent cell is safe. `topology` accepts `square` or pointy-top axial `hex`; `plane` maps local coordinates to `xy`, `xz`, or `yz`. The current grid is scene data only: a runtime host still needs to turn occupied cells into NexusGL draw commands.

The 3D constructor takes a row-major 4×4 view-projection matrix and caller-provided triangle vertices. It does not implement a camera system, model transforms, physics, animation, file import, or programmable shaders. NexusGL is a CPU renderer and reports GPU support as false. Surface dimensions are bounded to NexusGL's 8192-per-axis and 16,777,216-pixel limits.

The checked sample `examples/caravan_grid2d.camel` builds a 256×128 logical grid with three occupied cells. Grid dimensions do not allocate a dense cell array.

## 2D example

```camel
using "@camel/caravan/caravan.camel";

workflow CaravanCanvas2D Main() {
    CaravanCanvas2D canvas = NewCanvas2D(640, 360, MakeColor(22, 27, 36));
    canvas = AddDraw2D(canvas, MakeRectangle2D(0, 300, 640, 60, MakeColor(45, 45, 45)));
    canvas = AddDraw2D(canvas, MakeCircle2D(120, 250, 12, MakeColor(220, 190, 80)));
    return canvas;
}
```

## 3D example

Vertices are world-space, matching NexusGL's `SurfaceTriangle` contract. The identity projection is useful only for geometry already expressed in clip space; production scenes should supply their own valid matrix.

```camel
using "@camel/caravan/caravan.camel";

workflow CaravanScene3D Main() {
    CaravanColor paper = MakeColor(22, 27, 36);
    CaravanVector3D camera = MakeVector3D(0, 0, 5);
    CaravanVector3D light = MakeVector3D(-0.5, 1, 0.8);
    CaravanScene3D scene = NewScene3D(640, 360, paper, IdentityViewProjection3D(), camera, light);
    CaravanVector3D normal = MakeVector3D(0, 0, 1);
    CaravanVertex3D a = new CaravanVertex3D { position = MakeVector3D(-0.5, -0.5, 0), normal = normal };
    CaravanVertex3D b = new CaravanVertex3D { position = MakeVector3D(0.5, -0.5, 0), normal = normal };
    CaravanVertex3D c = new CaravanVertex3D { position = MakeVector3D(0, 0.5, 0), normal = normal };
    return AddTriangle3D(scene, MakeTriangle3D(a, b, c, MakeColor(180, 90, 60)));
}
```

## Library-manager testing

Caravan is listed in the checked-in catalog as a workspace-only `localPath` package. Studio and CLI can install it from the checkout or a distribution that bundles `libraries/caravan`; installation copies only `caravan.camel` and verifies its content SHA-256. This makes the package testable without publishing the private repository. Before publishing Caravan remotely, replace `localPath` and the content digest revision with the actual immutable commit that contains the final package, then verify remote installation. The checked examples at `examples/caravan_api.camel` and `examples/caravan_grid2d.camel` exercise the API.
