use rapier3d::prelude::*;

pub fn check_physics(clients: &mut Vec<(u8, [f32; 3], [f32; 3], [f32; 3], [f32; 3], [f32; 3], bool)>) {
    let mut rigid_body_set = RigidBodySet::new();
    let mut collider_set = ColliderSet::new();

    let mut handles = Vec::new();
    for (id, pos, vel, angvel, rot, hitbox, _) in clients.iter() {
        let collider = ColliderBuilder::cuboid(hitbox[0], hitbox[1], hitbox[2]).build();
        let rbody = RigidBodyBuilder::dynamic()
            .translation(vector![pos[0], pos[1], pos[2]])
            .rotation(vector![rot[0], rot[1], rot[2]])
            .linvel(vector![vel[0], vel[1], vel[2]])
            .angvel(vector![angvel[0], angvel[1], angvel[2]])
            .build();
        let rbody_handle = rigid_body_set.insert(rbody);
        let col_handle = collider_set.insert_with_parent(collider, rbody_handle, &mut rigid_body_set);
        handles.push((*id, col_handle, rbody_handle));
    }

    let gravity = vector![0.0, 0.0, 0.0];
    let integration_parameters = IntegrationParameters::default();
    let mut physics_pipeline = PhysicsPipeline::new();
    let mut island_manager = IslandManager::new();
    let mut broad_phase = BroadPhase::new();
    let mut narrow_phase = NarrowPhase::new();
    let mut impulse_joint_set = ImpulseJointSet::new();
    let mut multibody_joint_set = MultibodyJointSet::new();
    let mut ccd_solver = CCDSolver::new();
    let physics_hooks = ();
    let event_handler = ();

    for _ in 0..5 {
        physics_pipeline.step(
            &gravity,
            &integration_parameters,
            &mut island_manager,
            &mut broad_phase,
            &mut narrow_phase,
            &mut rigid_body_set,
            &mut collider_set,
            &mut impulse_joint_set,
            &mut multibody_joint_set,
            &mut ccd_solver,
            &physics_hooks,
            &event_handler,
        );

        for (id1, col_handle, rbody_handle) in &handles {
            for (id2, col_handle2, rbody_handle2) in &handles {
                if id1 == id2 { continue; }
                if let Some(contact_pair) = narrow_phase.contact_pair(*col_handle, *col_handle2) {
                    if contact_pair.has_any_active_contact {
                        for (id, _, vel, angvel, _, hbox, has_hit) in clients.iter_mut() {
                            let (linvel, rangvel) = if id == id1 {
                                (rigid_body_set[*rbody_handle].linvel(), rigid_body_set[*rbody_handle].angvel())
                            } else if id == id2 {
                                (rigid_body_set[*rbody_handle2].linvel(), rigid_body_set[*rbody_handle2].angvel())
                            } else {
                                continue;
                            };
                            if id == id1 || id == id2 {
                                *has_hit = true;
                                *vel = [linvel.x, linvel.y, linvel.z];
                                *angvel = [rangvel.x, rangvel.y, rangvel.z];
                            }
                        }
                    }
                }
            }
        }
    }
}
