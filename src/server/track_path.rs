use serde::Deserialize;

use crate::util::*;
use super::track_limits::{TrackLimits, Triangle};

#[derive(Deserialize)]
pub struct TrackPath {
    nodes: Vec<Node>,
    triangles: Vec<Triangle>,
    total_dist: f32,
}

#[derive(Deserialize, Copy, Clone)]
struct Node {
    pub p: [f32; 2],
    pub d: f32,
    pub t: f32,
}

impl TrackPath {
    pub fn get_angle_at_pos(&self, pos: [f32; 2]) -> f32 {
        let mut closest = 0;
        let mut closest_dist = (pos[0] - self.nodes[0].p[0]).powf(2.0) + (pos[1] - self.nodes[0].p[1]).powf(2.0);
        for i in 1..self.nodes.len() {
            let dist = (pos[0] - self.nodes[i].p[0]).powf(2.0) + (pos[1] - self.nodes[i].p[1]).powf(2.0);
            if dist < closest_dist {
                closest_dist = dist;
                closest = i;
            }
        }
        self.nodes[closest].d / std::f32::consts::PI * 180.0
    }

    pub fn get_percentage_along_track(&self, pos: [f32; 2]) -> f32 {
        let mut closest = 0;
        let mut closest_dist = (pos[0] - self.nodes[0].p[0]).powf(2.0) + (pos[1] - self.nodes[0].p[1]).powf(2.0);
        for i in 1..self.nodes.len() {
            let dist = (pos[0] - self.nodes[i].p[0]).powf(2.0) + (pos[1] - self.nodes[i].p[1]).powf(2.0);
            let car_node_angle = (self.nodes[i].p[1] - pos[1]).atan2(self.nodes[i].p[0] - pos[0]);
            let node_next_angle = (self.nodes[(i+1)%(self.nodes.len()-1)].p[1] - self.nodes[i].p[1]).atan2(self.nodes[(i+1)%(self.nodes.len()-1)].p[0] - self.nodes[i].p[0]);
            let car_ahead = ((car_node_angle - node_next_angle).abs() % 360.0) < 90.0;
            if dist < closest_dist && car_ahead {
                closest_dist = dist;
                closest = i;
            }
        }
        // self.nodes[closest].t - closest_dist / self.total_dist
        let finish = Node {
            p: self.nodes[0].p,
            d: self.nodes[0].d,
            t: 1.0,
        };
        let last = self.nodes[closest];
        let next = self.nodes.get(closest+1).unwrap_or(&finish); // In case we are at the last node, the next node is the start node with 100% completion
        let last_dist = distance(pos, last.p);
        let next_dist = distance(pos, next.p);
        let t_between = last_dist / (last_dist + next_dist);
        lerp32(&[last.t], &[next.t], t_between)[0]
    }

    pub fn check_limits(&self, client_pos: [f32; 2], client_size_half: [f32; 2]) -> bool {
        TrackLimits {
            triangles: self.triangles.clone(),
        }.check_limits(client_pos, client_size_half)
    }
}
