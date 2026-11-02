//! Marcadores, regiões e a região de loop.

use bv_core::{Tick, TimeRange};

use crate::model::{Marker, MarkerId, Project, Region};

impl Project {
    /// Insere um marcador em `time` e devolve o id. O nome padrão é o número.
    pub fn add_marker(&mut self, time: Tick) -> MarkerId {
        let id = MarkerId(self.alloc_id());
        let n = self.markers.len() + 1;
        self.markers.push(Marker {
            id,
            time: time.max(Tick::ZERO),
            name: n.to_string(),
        });
        self.markers.sort_by_key(|m| (m.time, m.id));
        id
    }

    pub fn add_region(&mut self, range: TimeRange) -> MarkerId {
        let id = MarkerId(self.alloc_id());
        let n = self.regions.len() + 1;
        self.regions.push(Region {
            id,
            range,
            name: n.to_string(),
        });
        self.regions.sort_by_key(|r| (r.range.start, r.id));
        id
    }

    pub fn move_marker(&mut self, id: MarkerId, time: Tick) {
        if let Some(m) = self.markers.iter_mut().find(|m| m.id == id) {
            m.time = time.max(Tick::ZERO);
        }
        self.markers.sort_by_key(|m| (m.time, m.id));
    }

    pub fn rename_marker(&mut self, id: MarkerId, name: String) {
        if let Some(m) = self.markers.iter_mut().find(|m| m.id == id) {
            m.name = name;
        } else if let Some(r) = self.regions.iter_mut().find(|r| r.id == id) {
            r.name = name;
        } else if let Some(t) = self.cd_tracks.iter_mut().find(|t| t.id == id) {
            t.name = name;
        }
    }

    /// Removes the marker, region, command marker, CD track or CD index
    /// with that id.
    pub fn remove_marker(&mut self, id: MarkerId) {
        self.markers.retain(|m| m.id != id);
        self.regions.retain(|r| r.id != id);
        self.commands.retain(|c| c.id != id);
        self.cd_tracks.retain(|t| t.id != id);
        self.cd_indices.retain(|i| i.id != id);
        self.sort_cd();
    }

    /// Próximo ponto de interesse depois de `t` (marcador, borda de região ou
    /// de clipe), para os atalhos "ir para o próximo".
    pub fn next_edit_point(&self, t: Tick) -> Option<Tick> {
        self.edit_points().into_iter().find(|&p| p > t)
    }

    pub fn previous_edit_point(&self, t: Tick) -> Option<Tick> {
        self.edit_points().into_iter().rev().find(|&p| p < t)
    }

    fn edit_points(&self) -> Vec<Tick> {
        let mut v: Vec<Tick> = self
            .markers
            .iter()
            .map(|m| m.time)
            .chain(
                self.regions
                    .iter()
                    .flat_map(|r| [r.range.start, r.range.end()]),
            )
            .chain(self.snap_points())
            .collect();
        v.sort();
        v.dedup();
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marcadores_ficam_ordenados() {
        let mut p = Project::default();
        let b = p.add_marker(Tick::from_seconds(5));
        let a = p.add_marker(Tick::from_seconds(2));
        assert_eq!(p.markers[0].id, a);
        p.move_marker(a, Tick::from_seconds(9));
        assert_eq!(p.markers[0].id, b);
        p.rename_marker(b, "Opening".into());
        assert_eq!(p.markers[0].name, "Opening");
        p.remove_marker(b);
        assert_eq!(p.markers.len(), 1);
    }

    #[test]
    fn pontos_de_edicao() {
        let mut p = Project::default();
        p.add_marker(Tick::from_seconds(3));
        p.add_region(TimeRange::new(Tick::from_seconds(5), Tick::from_seconds(2)));
        assert_eq!(p.next_edit_point(Tick::ZERO), Some(Tick::from_seconds(3)));
        assert_eq!(
            p.next_edit_point(Tick::from_seconds(3)),
            Some(Tick::from_seconds(5))
        );
        assert_eq!(
            p.previous_edit_point(Tick::from_seconds(6)),
            Some(Tick::from_seconds(5))
        );
        assert_eq!(p.next_edit_point(Tick::from_seconds(7)), None);
    }
}
