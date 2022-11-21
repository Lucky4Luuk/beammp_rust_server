use serde::Deserialize;

#[derive(Deserialize)]
pub struct TrackPath {
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
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
        // let mut closest = (0, 1);
        // let mut closest_dist = [9999.0, 9999.0];
        // let mut i = 0;
        // for pair in self.nodes.windows(2) {
        //     let dist_a = distance(pos, pair[0].p);
        //     let dist_b = distance(pos, pair[1].p);
        //     let dist_s = dist_a.min(dist_b);
        //     let dist_l = dist_a.max(dist_b);
        //     if dist_s < closest_dist[0] && dist_l < closest_dist[1] {
        //         closest_dist = [dist_s, dist_l];
        //         closest = (i, i+1);
        //     }
        //     i += 1;
        // }
        // let total = closest_dist[0] + closest_dist[1];
        // let (first, last) = if closest_dist[0] < closest_dist[1] {
        //     (closest_dist[0], closest_dist[1])
        // } else {
        //     (closest_dist[1], closest_dist[0])
        // };
        // let t0 = first / total;
        // let t1 = last / total;
        // first * t0 + last * t1
        let mut closest = 0;
        let mut closest_dist = (pos[0] - self.nodes[0].p[0]).powf(2.0) + (pos[1] - self.nodes[0].p[1]).powf(2.0);
        for i in 1..self.nodes.len() {
            let dist = (pos[0] - self.nodes[i].p[0]).powf(2.0) + (pos[1] - self.nodes[i].p[1]).powf(2.0);
            if dist < closest_dist {
                closest_dist = dist;
                closest = i;
            }
        }
        self.nodes[closest].t
    }
}

fn distance(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((b[0] - a[0]).powf(2.0) - (b[1] - a[1]).powf(2.0)).sqrt()
}
