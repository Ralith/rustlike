use std::cmp::Reverse;
use std::f32;

use na::allocator::Allocator;
use ordered_float::NotNaN;
use radix_heap::RadixHeapMap;

#[derive(Default, Clone)]
pub struct NavMesh {
    nodes: Vec<Node>,
}

impl NavMesh {
    pub fn new(nodes: Vec<Node>) -> Self {
        Self { nodes }
    }

    pub fn plan(
        &self,
        start_node: u32,
        start: &na::Point2<f32>,
        goal_node: u32,
        goal: &na::Point2<f32>,
    ) -> Vec<na::Point2<f32>> {
        let channel = self.plan_channel(start_node, goal_node, goal);
        refine_path(start, &channel)
    }

    /// Compute a sequence of edges to traverse using A*
    fn plan_channel(
        &self,
        start_node: u32,
        goal_node: u32,
        goal: &na::Point2<f32>,
    ) -> Vec<[na::Point2<f32>; 2]> {
        let mut frontier = RadixHeapMap::new_at(Reverse(NotNaN::new(0.0).unwrap()));
        frontier.push(Reverse(NotNaN::new(0.0).unwrap()), start_node);
        let mut came_from: Vec<Option<(u32, u32)>> = vec![None; self.nodes.len()];
        let mut cost = vec![f32::INFINITY; self.nodes.len()];
        cost[start_node as usize] = 0.0;

        while let Some((_, current)) = frontier.pop() {
            if current == goal_node {
                break;
            }
            for (i, next) in self.nodes[current as usize].edges.iter().enumerate() {
                let next = next.neighbor;
                let next_cost = cost[current as usize] + self.edge_cost(current, i);
                if next_cost >= cost[next as usize] {
                    continue;
                }
                cost[next as usize] = next_cost;
                frontier.push(
                    Reverse(NotNaN::new(next_cost + self.heuristic(next, goal)).expect("NaN")),
                    next,
                );
                came_from[next as usize] = Some((current, i as u32));
            }
        }

        let mut result = vec![[*goal, *goal]];
        let mut node = goal_node;
        while node != start_node {
            let (prev, prev_edge) = came_from[node as usize].expect("unconnected graph");
            result.push(self.nodes[prev as usize].edges[prev_edge as usize].vertices);
            node = prev;
        }
        result.reverse();
        result
    }

    fn edge_cost(&self, node: u32, edge: usize) -> f32 {
        let node = &self.nodes[node as usize];
        let neighbor = &self.nodes[node.edges[edge].neighbor as usize];
        na::distance(&node.center, &neighbor.center)
    }

    fn heuristic(&self, node: u32, goal: &na::Point2<f32>) -> f32 {
        self.nodes[node as usize]
            .edges
            .iter()
            .map(|x| {
                // TODO: Distance to line segment
                na::distance_squared(&x.vertices[0], goal).min(na::distance(&x.vertices[1], goal))
            })
            .fold(f32::INFINITY, |x, y| x.min(y))
    }
}

fn refine_path(start: &na::Point2<f32>, channel: &[[na::Point2<f32>; 2]]) -> Vec<na::Point2<f32>> {
    // https://digestingduck.blogspot.com/2010/03/simple-stupid-funnel-algorithm.html
    // https://skatgame.net/mburo/ps/thesis_demyen_2006.pdf
    let mut apex = start;
    let mut left_index = 0;
    let mut right_index = 0;
    let mut i = 1;
    let mut result = Vec::new();
    while i < channel.len() {
        let portal = &channel[i];
        let left = &channel[left_index][0];
        let right = &channel[right_index][1];
        // If new left edge is inside on the right
        if area2(apex, &portal[0], right) >= 0.0 {
            // If the new left edge is inside on the left
            if area2(apex, left, &portal[0]) >= 0.0 {
                // Narrow the funnel
                left_index = i;
            }
        } else {
            apex = right;
            result.push(*apex);
            left_index = right_index + 1;
            right_index = right_index + 1;
            i = right_index + 2;
            continue;
        }
        // If new right edge is inside on the left
        if area2(apex, left, &portal[1]) >= 0.0 {
            // If the new right edge is inside on the right
            if area2(apex, &portal[1], right) >= 0.0 {
                // Narrow the funnel
                left_index = i;
            }
        } else {
            apex = left;
            result.push(*apex);
            left_index = left_index + 1;
            right_index = left_index + 1;
            i = right_index + 2;
            continue;
        }
        i += 1;
    }

    result
}

/// Compute two times the signed area of a triangle
fn area2(a: &na::Point2<f32>, b: &na::Point2<f32>, c: &na::Point2<f32>) -> f32 {
    let b = b - a;
    let c = c - a;
    b.x * c.y - c.x * b.y
}

#[derive(Clone)]
pub struct Node {
    center: na::Point2<f32>,
    edges: Vec<Edge>,
}

#[derive(Clone)]
pub struct Edge {
    vertices: [na::Point2<f32>; 2],
    neighbor: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn area_signs() {
        assert!(
            area2(
                &na::Point2::origin(),
                &na::Point2::new(-1.0, 1.0),
                &na::Point2::new(1.0, 1.0)
            ) < 0.0
        );
        assert!(
            area2(
                &na::Point2::origin(),
                &na::Point2::new(1.0, 1.0),
                &na::Point2::new(-1.0, 1.0)
            ) > 0.0
        );
    }

    #[test]
    fn empty() {
        let mesh = NavMesh::new(vec![Node {
            center: na::Point2::origin(),
            edges: vec![],
        }]);
        let channel = mesh.plan_channel(0, 0, &na::Point2::origin());
        assert_eq!(channel.len(), 1);
    }

    #[test]
    fn right_corner() {
        // ---+
        // --+|
        //   ||
        let mesh = NavMesh::new(vec![
            Node {
                center: na::Point2::origin(),
                edges: vec![Edge {
                    vertices: [na::Point2::new(9.0, 0.0), na::Point2::new(10.0, 1.0)],
                    neighbor: 1,
                }],
            },
            Node {
                center: na::Point2::new(9.5, -5.0),
                edges: vec![Edge {
                    vertices: [na::Point2::new(10.0, 1.0), na::Point2::new(9.0, 0.0)],
                    neighbor: 0,
                }],
            },
        ]);
        let path = mesh.plan(0, &na::Point2::origin(), 1, &na::Point2::new(9.5, -5.0));
        assert_eq!(path[..], [na::Point2::new(9.0, 0.0)][..]);
    }

    #[test]
    fn left_corner() {
        //   ||
        // --+|
        // ---+
        let mesh = NavMesh::new(vec![
            Node {
                center: na::Point2::origin(),
                edges: vec![Edge {
                    vertices: [na::Point2::new(10.0, 0.0), na::Point2::new(9.0, 1.0)],
                    neighbor: 1,
                }],
            },
            Node {
                center: na::Point2::new(9.5, 5.0),
                edges: vec![Edge {
                    vertices: [na::Point2::new(9.0, 1.0), na::Point2::new(10.0, 0.0)],
                    neighbor: 0,
                }],
            },
        ]);
        let path = mesh.plan(0, &na::Point2::origin(), 1, &na::Point2::new(9.5, 5.0));
        assert_eq!(path[..], [na::Point2::new(9.0, 1.0)][..]);
    }

    #[test]
    fn straight() {
        // --+--
        // --+--
        let mesh = NavMesh::new(vec![
            Node {
                center: na::Point2::origin(),
                edges: vec![Edge {
                    vertices: [na::Point2::new(10.0, -1.0), na::Point2::new(10.0, 1.0)],
                    neighbor: 1,
                }],
            },
            Node {
                center: na::Point2::new(20.0, 0.0),
                edges: vec![Edge {
                    vertices: [na::Point2::new(10.0, 1.0), na::Point2::new(10.0, -1.0)],
                    neighbor: 0,
                }],
            },
        ]);
        let path = mesh.plan(0, &na::Point2::origin(), 1, &na::Point2::new(20.0, 0.0));
        assert_eq!(path.len(), 0);
    }

    #[test]
    fn multi_edge() {
        // -----
        //
        //   |
        //
        // -----
        let mesh = NavMesh::new(vec![
            Node {
                center: na::Point2::origin(),
                edges: vec![
                    Edge {
                        vertices: [na::Point2::new(0.0, 1.0), na::Point2::new(0.0, 2.0)],
                        neighbor: 1,
                    },
                    Edge {
                        vertices: [na::Point2::new(0.0, -2.0), na::Point2::new(0.0, -1.0)],
                        neighbor: 1,
                    },
                ],
            },
            Node {
                center: na::Point2::new(20.0, 0.0),
                edges: vec![
                    Edge {
                        vertices: [na::Point2::new(0.0, 2.0), na::Point2::new(0.0, 1.0)],
                        neighbor: 1,
                    },
                    Edge {
                        vertices: [na::Point2::new(0.0, -1.0), na::Point2::new(0.0, -2.0)],
                        neighbor: 1,
                    },
                ],
            },
        ]);
        let path = mesh.plan(
            0,
            &na::Point2::new(-1.0, 0.0),
            1,
            &na::Point2::new(1.0, 0.0),
        );
        assert_eq!(path.len(), 1);
    }
}
