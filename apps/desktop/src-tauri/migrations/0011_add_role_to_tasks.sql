-- El rol de una tarea es trazabilidad, no una restricción: al crear la tarea
-- se copian sus valores (perfil, modelo, ámbito) y el usuario puede ajustarlos.
ALTER TABLE tasks ADD COLUMN role_id INTEGER REFERENCES roles (id) ON DELETE SET NULL;
