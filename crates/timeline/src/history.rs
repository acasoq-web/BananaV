/// Desfazer/refazer por fotografias do estado.
///
/// Cada edição guarda uma cópia do estado anterior. Para um projeto de
/// edição (algumas centenas de clipes) a cópia custa microssegundos, e o
/// modelo fica livre de comandos inversos difíceis de acertar.
#[derive(Clone, Debug)]
pub struct History<T: Clone> {
    current: T,
    undo: Vec<T>,
    redo: Vec<T>,
    limit: usize,
    /// Chave da última edição "agrupável" (ver [`History::apply_merged`]).
    merge_key: Option<u64>,
    /// How many edits were recorded, and how many times undo, redo or a
    /// reset moved the state (see [`History::edits`]).
    edits: u64,
    moves: u64,
    /// Undo steps recorded, less those joined (see [`History::join_since`]).
    stacked: u64,
}

impl<T: Clone + PartialEq> History<T> {
    pub fn new(initial: T) -> History<T> {
        History {
            current: initial,
            undo: Vec::new(),
            redo: Vec::new(),
            limit: 500,
            merge_key: None,
            edits: 0,
            moves: 0,
            stacked: 0,
        }
    }

    /// How many edits were recorded so far (each `apply`, merged run or
    /// gesture that changed the state): what follows the edits — new
    /// tracks' default properties — tells them from undo, redo and resets
    /// ([`History::moves`]).
    pub fn edits(&self) -> u64 {
        self.edits
    }

    /// A mark of the undo steps so far, for [`History::join_since`].
    pub fn undo_mark(&self) -> u64 {
        self.stacked
    }

    /// Makes the undo steps recorded since `mark` ([`History::undo_mark`])
    /// one (files dropped together: their imports and events), the state
    /// before the first of them its undo. Marks may nest.
    pub fn join_since(&mut self, mark: u64) {
        let n = self.stacked.saturating_sub(mark) as usize;
        if n > 1 {
            let len = self.undo.len();
            let from = len.saturating_sub(n - 1).max(1).min(len);
            self.undo.drain(from..);
            self.stacked -= (len - from) as u64;
            self.merge_key = None;
        }
    }

    /// How many times undo, redo or a reset replaced the state.
    pub fn moves(&self) -> u64 {
        self.moves
    }

    pub fn current(&self) -> &T {
        &self.current
    }

    /// Aplica uma edição. Se ela falhar ou não mudar nada, nada entra no histórico.
    pub fn apply<R, E>(&mut self, edit: impl FnOnce(&mut T) -> Result<R, E>) -> Result<R, E> {
        self.merge_key = None;
        let before = self.current.clone();
        match edit(&mut self.current) {
            Ok(r) => {
                if self.current != before {
                    self.push_undo(before);
                }
                Ok(r)
            }
            Err(e) => {
                self.current = before;
                Err(e)
            }
        }
    }

    /// Aplica uma edição que se junta às anteriores com a mesma `key` (ex.:
    /// arrastar um controle deslizante): só a primeira cria entrada de
    /// desfazer. Qualquer outra edição encerra o agrupamento.
    pub fn apply_merged<R>(&mut self, key: u64, edit: impl FnOnce(&mut T) -> R) -> R {
        if self.merge_key == Some(key) && self.redo.is_empty() {
            return edit(&mut self.current);
        }
        let before = self.current.clone();
        let r = edit(&mut self.current);
        if self.current != before {
            self.push_undo(before);
            self.merge_key = Some(key);
        }
        r
    }

    /// Applies a change outside the history: to the state and to every state
    /// undo and redo go back to, so that neither brings the old value back
    /// (a change that makes no undo of its own). Where a state has nothing
    /// for it to change, `edit` leaves it as it is.
    pub fn apply_everywhere(&mut self, edit: impl Fn(&mut T)) {
        edit(&mut self.current);
        for s in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            edit(s);
        }
    }

    /// Changes the state without an undo of its own: the change belongs to
    /// the edit that made it needed (Auto Normalize's gain after a trim),
    /// and undo goes back past both.
    pub fn amend(&mut self, edit: impl FnOnce(&mut T)) {
        edit(&mut self.current);
    }

    /// Começa uma edição contínua (ex.: arrastar um clipe): o estado de agora
    /// vira o ponto de desfazer, e [`preview`](Self::preview) altera o estado
    /// sem criar novas entradas até [`commit_gesture`](Self::commit_gesture).
    pub fn begin_gesture(&mut self) -> Gesture<T> {
        Gesture {
            before: self.current.clone(),
        }
    }

    /// Altera o estado durante um gesto, sem registrar no histórico.
    pub fn preview(&mut self, gesture: &Gesture<T>, edit: impl FnOnce(&mut T)) {
        self.current = gesture.before.clone();
        edit(&mut self.current);
    }

    pub fn commit_gesture(&mut self, gesture: Gesture<T>) {
        self.merge_key = None;
        if self.current != gesture.before {
            self.push_undo(gesture.before);
        }
    }

    pub fn cancel_gesture(&mut self, gesture: Gesture<T>) {
        self.current = gesture.before;
    }

    fn push_undo(&mut self, before: T) {
        self.edits += 1;
        self.stacked += 1;
        self.undo.push(before);
        if self.undo.len() > self.limit {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// The state an undo would return to.
    pub fn undo_target(&self) -> Option<&T> {
        self.undo.last()
    }

    /// The state a redo would restore.
    pub fn redo_target(&self) -> Option<&T> {
        self.redo.last()
    }

    /// Undoes every recorded edit (they stay available for redo).
    pub fn undo_all(&mut self) -> bool {
        let mut any = false;
        while self.undo() {
            any = true;
        }
        any
    }

    /// Forgets the undo/redo history, keeping the current state.
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.merge_key = None;
    }

    /// How many edits can be undone.
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) -> bool {
        self.merge_key = None;
        match self.undo.pop() {
            Some(prev) => {
                self.moves += 1;
                self.redo.push(std::mem::replace(&mut self.current, prev));
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        self.merge_key = None;
        match self.redo.pop() {
            Some(next) => {
                self.moves += 1;
                self.undo.push(std::mem::replace(&mut self.current, next));
                true
            }
            None => false,
        }
    }

    /// Substitui o estado sem histórico (ex.: ao abrir um projeto).
    pub fn reset(&mut self, state: T) {
        self.moves += 1;
        self.current = state;
        self.undo.clear();
        self.redo.clear();
    }
}

/// Uma edição contínua em andamento. Ver [`History::begin_gesture`].
#[derive(Debug)]
pub struct Gesture<T> {
    before: T,
}

impl<T> Gesture<T> {
    /// O estado de quando o gesto começou.
    pub fn before(&self) -> &T {
        &self.before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_everywhere_survives_undo_and_redo() {
        let mut h = History::new(vec![0, 0]);
        let set = |i: i32| {
            move |v: &mut Vec<i32>| {
                v[0] = i;
                Ok::<_, ()>(())
            }
        };
        h.apply(set(1)).unwrap();
        h.apply(set(2)).unwrap();
        h.undo();
        h.apply_everywhere(|v| v[1] = 7);
        assert_eq!(h.current(), &vec![1, 7]);
        h.undo();
        assert_eq!(h.current(), &vec![0, 7], "undo keeps it");
        h.redo();
        h.redo();
        assert_eq!(h.current(), &vec![2, 7], "redo keeps it");
    }

    #[test]
    fn edits_joined_undo_together() {
        let mut h = History::new(0);
        let add = |n: i32| {
            move |v: &mut i32| {
                *v += n;
                Ok::<_, ()>(())
            }
        };
        h.apply(add(1)).unwrap();
        let mark = h.undo_mark();
        h.apply(add(10)).unwrap();
        let inner = h.undo_mark();
        h.apply(add(100)).unwrap();
        h.apply(add(1000)).unwrap();
        h.join_since(inner);
        assert_eq!(h.undo_len(), 3, "the inner two joined");
        h.join_since(mark);
        assert_eq!(h.undo_len(), 2);
        assert!(h.undo());
        assert_eq!(*h.current(), 1, "the three undo at once");
        assert!(h.redo());
        assert_eq!(*h.current(), 1111);
        let mark = h.undo_mark();
        h.apply(add(2)).unwrap();
        h.join_since(mark);
        assert_eq!(h.undo_len(), 3, "one edit stays as it was");
    }

    #[test]
    fn desfazer_e_refazer() {
        let mut h = History::new(0);
        h.apply(|v| {
            let _: () = *v += 1;
            Ok::<_, ()>(())
        })
        .unwrap();
        h.apply(|v| {
            let _: () = *v += 10;
            Ok::<_, ()>(())
        })
        .unwrap();
        assert_eq!(*h.current(), 11);
        assert!(h.undo());
        assert_eq!(*h.current(), 1);
        assert!(h.redo());
        assert_eq!(*h.current(), 11);
        assert!(!h.redo());
        h.undo();
        h.apply(|v| {
            let _: () = *v = 5;
            Ok::<_, ()>(())
        })
        .unwrap();
        assert!(!h.can_redo(), "nova edição descarta o refazer");
    }

    #[test]
    fn falha_e_edicao_vazia_nao_entram_no_historico() {
        let mut h = History::new(0);
        assert!(
            h.apply(|v| {
                *v = 9;
                Err::<(), _>("x")
            })
            .is_err()
        );
        assert_eq!(*h.current(), 0);
        h.apply(|_| Ok::<_, ()>(())).unwrap();
        assert!(!h.can_undo());
    }

    #[test]
    fn edicoes_agrupadas_viram_uma_entrada() {
        let mut h = History::new(0);
        for v in 1..=10 {
            h.apply_merged(7, |x| *x = v);
        }
        h.apply_merged(8, |x| *x = 100);
        assert_eq!(*h.current(), 100);
        h.undo();
        assert_eq!(*h.current(), 10);
        h.undo();
        assert_eq!(*h.current(), 0);
    }

    #[test]
    fn gesto_vira_uma_entrada() {
        let mut h = History::new(0);
        let g = h.begin_gesture();
        for x in 1..=5 {
            h.preview(&g, |v| *v = x);
        }
        h.commit_gesture(g);
        assert_eq!(*h.current(), 5);
        h.undo();
        assert_eq!(*h.current(), 0);
        let g = h.begin_gesture();
        h.preview(&g, |v| *v = 7);
        h.cancel_gesture(g);
        assert_eq!(*h.current(), 0);
    }
}
