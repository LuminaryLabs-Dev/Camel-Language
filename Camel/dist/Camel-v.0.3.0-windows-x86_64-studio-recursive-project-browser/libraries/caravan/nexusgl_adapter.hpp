#pragma once

#include <nexusgl/graphics.hpp>

#include <array>
#include <cmath>
#include <cstdint>
#include <stdexcept>
#include <string>
#include <type_traits>
#include <variant>
#include <vector>

// C++ host-side adapter for values decoded from caravan.camel records.
// Rendering remains owned by NexusGL's Renderer and scene descriptors.
namespace caravan {

struct Color { int r = 0, g = 0, b = 0, a = 255; };
struct Point2D { double x = 0, y = 0; };
struct Vector3D { double x = 0, y = 0, z = 0; };
struct Triangle2D { std::array<Point2D, 3> vertices{}; Color color{}; };
struct Rectangle2D { double x = 0, y = 0, width = 0, height = 0; Color color{}; };
struct Circle2D { double x = 0, y = 0, radius = 0; Color color{}; };
using Draw2D = std::variant<Triangle2D, Rectangle2D, Circle2D>;

struct Canvas2D {
    int width = 0, height = 0;
    Color background{};
    std::vector<Draw2D> draws;
};

struct GridCell2D {
    int q = 0, r = 0;
    std::string tileId;
    Color color{};
};
struct Grid2D {
    int columns = 0, rows = 0;
    double cellSize = 0;
    std::string topology = "square";
    std::string plane = "xy";
    Color background{};
    std::vector<GridCell2D> cells;
};

struct Vertex3D { Vector3D position{}; Vector3D normal{0, 0, 1}; };
struct Triangle3D {
    std::array<Vertex3D, 3> vertices{};
    Color albedo{};
    bool castsShadow = true;
    bool twoSidedShadow = false;
};
struct Scene3D {
    int width = 0, height = 0;
    Color background{};
    std::array<double, 16> viewProjection{};
    Vector3D cameraPosition{0, 0, 5};
    Vector3D directionToLight{-0.5, 1, 0.8};
    std::vector<Triangle3D> triangles;
};

inline nexusgl::Color toNexus(Color color) {
    const auto valid = [](int channel) { return channel >= 0 && channel <= 255; };
    if (!valid(color.r) || !valid(color.g) || !valid(color.b) || !valid(color.a)) {
        throw std::invalid_argument("Caravan color channels must be in 0..255");
    }
    return {static_cast<std::uint8_t>(color.r), static_cast<std::uint8_t>(color.g),
            static_cast<std::uint8_t>(color.b), static_cast<std::uint8_t>(color.a)};
}

inline nexusgl::Vec3 toNexus(Vector3D value) { return {value.x, value.y, value.z}; }
inline nexusgl::Point toNexus(Point2D value) { return {value.x, value.y}; }

inline void validate(const Grid2D& grid) {
    if (grid.columns <= 0 || grid.columns > 1000000 || grid.rows <= 0 || grid.rows > 1000000
        || !(grid.cellSize > 0) || !std::isfinite(grid.cellSize)) {
        throw std::invalid_argument("Caravan grid dimensions and cell size must be positive");
    }
    if (grid.topology != "square" && grid.topology != "hex") {
        throw std::invalid_argument("Caravan grid topology must be square or hex");
    }
    if (grid.plane != "xy" && grid.plane != "xz" && grid.plane != "yz") {
        throw std::invalid_argument("Caravan grid plane must be xy, xz, or yz");
    }
    for (std::size_t i = 0; i < grid.cells.size(); ++i) {
        const auto& cell = grid.cells[i];
        if (cell.q < 0 || cell.q >= grid.columns || cell.r < 0 || cell.r >= grid.rows) {
            throw std::invalid_argument("Caravan grid cell is out of bounds");
        }
        if (cell.tileId.empty()) {
            throw std::invalid_argument("Caravan grid tile id must not be empty");
        }
        (void)toNexus(cell.color);
        for (std::size_t j = 0; j < i; ++j) {
            if (grid.cells[j].q == cell.q && grid.cells[j].r == cell.r) {
                throw std::invalid_argument("Caravan grid contains duplicate cell coordinates");
            }
        }
    }
}

inline Point2D gridCenter2D(const Grid2D& grid, int q, int r) {
    if (grid.topology == "square") {
        return {(q + 0.5) * grid.cellSize, (r + 0.5) * grid.cellSize};
    }
    constexpr double root3 = 1.7320508075688772;
    return {root3 * grid.cellSize * (q + r * 0.5), 1.5 * grid.cellSize * r};
}

inline Vector3D gridCenter3D(const Grid2D& grid, int q, int r) {
    const auto point = gridCenter2D(grid, q, r);
    if (grid.plane == "xy") return {point.x, point.y, 0};
    if (grid.plane == "xz") return {point.x, 0, point.y};
    return {0, point.x, point.y};
}

inline nexusgl::scene::CanvasFrame toNexus(const Canvas2D& canvas) {
    if (canvas.width <= 0 || canvas.height <= 0) {
        throw std::invalid_argument("Caravan canvas dimensions must be positive");
    }
    nexusgl::scene::CanvasFrame frame;
    frame.background = toNexus(canvas.background);
    for (const auto& draw : canvas.draws) {
        std::visit([&](const auto& shape) {
            using Shape = std::decay_t<decltype(shape)>;
            if constexpr (std::is_same_v<Shape, Triangle2D>) {
                frame.draws.emplace_back(nexusgl::scene::Triangle2D{{
                    toNexus(shape.vertices[0]), toNexus(shape.vertices[1]),
                    toNexus(shape.vertices[2])}, toNexus(shape.color)});
            } else if constexpr (std::is_same_v<Shape, Rectangle2D>) {
                frame.draws.emplace_back(nexusgl::scene::Rectangle2D{
                    shape.x, shape.y, shape.width, shape.height, toNexus(shape.color)});
            } else {
                frame.draws.emplace_back(nexusgl::scene::Circle2D{
                    {shape.x, shape.y}, shape.radius, toNexus(shape.color)});
            }
        }, draw);
    }
    return frame;
}

inline nexusgl::scene::CanvasFrame toNexus(const Grid2D& grid) {
    validate(grid);
    if (grid.plane != "xy") {
        throw std::invalid_argument("2D canvas rendering requires the xy grid plane");
    }
    constexpr double pi = 3.14159265358979323846;
    nexusgl::scene::CanvasFrame frame;
    frame.background = toNexus(grid.background);
    for (const auto& cell : grid.cells) {
        const auto center = gridCenter2D(grid, cell.q, cell.r);
        const auto color = toNexus(cell.color);
        if (grid.topology == "square") {
            frame.draws.emplace_back(nexusgl::scene::Rectangle2D{
                cell.q * grid.cellSize, cell.r * grid.cellSize,
                grid.cellSize, grid.cellSize, color});
        } else {
            for (int corner = 0; corner < 6; ++corner) {
                const double a = -pi / 2.0 + corner * pi / 3.0;
                const double b = -pi / 2.0 + (corner + 1) * pi / 3.0;
                nexusgl::scene::Triangle2D triangle;
                triangle.vertices = {
                    nexusgl::Point{center.x, center.y},
                    nexusgl::Point{center.x + grid.cellSize * std::cos(a), center.y + grid.cellSize * std::sin(a)},
                    nexusgl::Point{center.x + grid.cellSize * std::cos(b), center.y + grid.cellSize * std::sin(b)}
                };
                triangle.color = color;
                frame.draws.emplace_back(triangle);
            }
        }
    }
    return frame;
}

inline nexusgl::scene::SceneFrame toNexus(
    const Grid2D& grid,
    const std::array<double, 16>& viewProjection,
    Vector3D cameraPosition,
    Vector3D directionToLight
) {
    validate(grid);
    constexpr double pi = 3.14159265358979323846;
    nexusgl::scene::SceneFrame frame;
    frame.background = toNexus(grid.background);
    frame.viewProjection.values = viewProjection;
    frame.cameraPosition = toNexus(cameraPosition);
    frame.shadow.directionToLight = toNexus(directionToLight);
    const auto makePoint = [&](double u, double v) {
        if (grid.plane == "xy") return Vector3D{u, v, 0};
        if (grid.plane == "xz") return Vector3D{u, 0, v};
        return Vector3D{0, u, v};
    };
    const Vector3D normal = grid.plane == "xy" ? Vector3D{0, 0, 1}
        : grid.plane == "xz" ? Vector3D{0, -1, 0} : Vector3D{1, 0, 0};
    const auto appendTriangle = [&](Vector3D a, Vector3D b, Vector3D c, Color color) {
        nexusgl::scene::SurfaceTriangle triangle;
        triangle.albedo = toNexus(color);
        triangle.castsShadow = false;
        triangle.geometricNormal = toNexus(normal);
        const std::array<Vector3D, 3> positions{a, b, c};
        for (std::size_t i = 0; i < positions.size(); ++i) {
            triangle.vertices[i].position = toNexus(positions[i]);
            triangle.vertices[i].normal = toNexus(normal);
        }
        frame.triangles.push_back(std::move(triangle));
    };
    for (const auto& cell : grid.cells) {
        const auto center = gridCenter2D(grid, cell.q, cell.r);
        if (grid.topology == "square") {
            const double u = cell.q * grid.cellSize;
            const double v = cell.r * grid.cellSize;
            const auto p0 = makePoint(u, v);
            const auto p1 = makePoint(u + grid.cellSize, v);
            const auto p2 = makePoint(u + grid.cellSize, v + grid.cellSize);
            const auto p3 = makePoint(u, v + grid.cellSize);
            appendTriangle(p0, p1, p2, cell.color);
            appendTriangle(p0, p2, p3, cell.color);
        } else {
            const auto origin = makePoint(center.x, center.y);
            for (int corner = 0; corner < 6; ++corner) {
                const double a = -pi / 2.0 + corner * pi / 3.0;
                const double b = -pi / 2.0 + (corner + 1) * pi / 3.0;
                appendTriangle(
                    origin,
                    makePoint(center.x + grid.cellSize * std::cos(a), center.y + grid.cellSize * std::sin(a)),
                    makePoint(center.x + grid.cellSize * std::cos(b), center.y + grid.cellSize * std::sin(b)),
                    cell.color);
            }
        }
    }
    return frame;
}

inline nexusgl::scene::SceneFrame toNexus(const Scene3D& scene) {
    if (scene.width <= 0 || scene.height <= 0) {
        throw std::invalid_argument("Caravan scene dimensions must be positive");
    }
    nexusgl::scene::SceneFrame frame;
    frame.background = toNexus(scene.background);
    frame.viewProjection.values = scene.viewProjection;
    frame.cameraPosition = toNexus(scene.cameraPosition);
    frame.shadow.directionToLight = toNexus(scene.directionToLight);
    frame.triangles.reserve(scene.triangles.size());
    for (const auto& triangle : scene.triangles) {
        nexusgl::scene::SurfaceTriangle surface;
        surface.albedo = toNexus(triangle.albedo);
        surface.castsShadow = triangle.castsShadow;
        surface.twoSidedShadow = triangle.twoSidedShadow;
        for (std::size_t i = 0; i < triangle.vertices.size(); ++i) {
            surface.vertices[i].position = toNexus(triangle.vertices[i].position);
            surface.vertices[i].normal = toNexus(triangle.vertices[i].normal);
        }
        const auto& a = surface.vertices[0].position;
        const auto& b = surface.vertices[1].position;
        const auto& c = surface.vertices[2].position;
        const nexusgl::Vec3 u{b.x-a.x, b.y-a.y, b.z-a.z};
        const nexusgl::Vec3 v{c.x-a.x, c.y-a.y, c.z-a.z};
        surface.geometricNormal = {u.y*v.z-u.z*v.y, u.z*v.x-u.x*v.z, u.x*v.y-u.y*v.x};
        frame.triangles.push_back(surface);
    }
    return frame;
}

// Returned framebuffer references are borrowed until the next renderer call.
inline const nexusgl::Framebuffer& render(nexusgl::Renderer& renderer, const Canvas2D& canvas) {
    auto frame = toNexus(canvas);
    return renderer.render(frame, {canvas.width, canvas.height});
}

inline const nexusgl::Framebuffer& render(
    nexusgl::Renderer& renderer, const Grid2D& grid, nexusgl::Surface surface
) {
    auto frame = toNexus(grid);
    return renderer.render(frame, surface);
}

inline const nexusgl::Framebuffer& render(
    nexusgl::Renderer& renderer,
    const Grid2D& grid,
    nexusgl::Surface surface,
    const std::array<double, 16>& viewProjection,
    Vector3D cameraPosition,
    Vector3D directionToLight
) {
    auto frame = toNexus(grid, viewProjection, cameraPosition, directionToLight);
    return renderer.render(frame, surface);
}

inline const nexusgl::Framebuffer& render(nexusgl::Renderer& renderer, const Scene3D& scene) {
    auto frame = toNexus(scene);
    return renderer.render(frame, {scene.width, scene.height});
}

} // namespace caravan
