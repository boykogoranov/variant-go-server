use crate::game::{
    find_groups, ActionChange, ActionKind, Board, Color, GameState, Group, GroupVec,
    MakeActionResult, Point, Seat, SharedState,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoringState {
    pub groups: Vec<Group>,
    /// Vector of the board, marking who owns a point
    pub points: Board,
    pub scores: GroupVec<i32>,
    // TODO: use smallvec?
    pub players_accepted: Vec<bool>,
}

impl ScoringState {
    pub fn new(board: &Board, seats: &[Seat], scores: &[i32]) -> Self {
        let groups = find_groups(board);
        let points = score_board(board, &groups);
        let mut scores: GroupVec<i32> = scores.into();
        for color in &points.points {
            if !color.is_empty() {
                scores[color.0 as usize - 1] += 2;
            }
        }
        ScoringState {
            groups,
            points,
            scores,
            players_accepted: seats.iter().map(|s| s.resigned).collect(),
        }
    }

    pub fn make_action_place(
        &mut self,
        shared: &mut SharedState,
        point: Point,
    ) -> MakeActionResult {
        let group = self.groups.iter_mut().find(|g| g.points.contains(&point));

        let group = match group {
            Some(g) => g,
            None => return Ok(ActionChange::None),
        };

        group.alive = !group.alive;

        self.points = score_board(&shared.board, &self.groups);
        self.scores = shared.points.clone();
        for color in &self.points.points {
            if !color.is_empty() {
                self.scores[color.0 as usize - 1] += 2;
            }
        }

        for (idx, accept) in self.players_accepted.iter_mut().enumerate() {
            *accept = shared.seats[idx].resigned;
        }

        Ok(ActionChange::None)
    }

    pub fn make_action_pass(
        &mut self,
        shared: &mut SharedState,
        player_id: u64,
    ) -> MakeActionResult {
        // A single player can hold multiple seats so we have to mark every seat they hold
        let seats = shared
            .seats
            .iter()
            .enumerate()
            .filter(|x| x.1.player == Some(player_id));

        for (seat_idx, _) in seats {
            self.players_accepted[seat_idx] = true;
        }
        if self.players_accepted.iter().all(|x| *x) {
            Ok(ActionChange::SwapState(GameState::Done(self.clone())))
        } else {
            Ok(ActionChange::None)
        }
    }

    fn make_action_resign(&mut self, shared: &mut SharedState, player_id: u64) -> MakeActionResult {
        // A single player can hold multiple seats so we have to mark every seat they hold
        let seats = shared
            .seats
            .iter_mut()
            .enumerate()
            .filter(|x| x.1.player == Some(player_id));

        for (seat_idx, seat) in seats {
            seat.resigned = true;
            self.players_accepted[seat_idx] = true;
        }

        if self.players_accepted.iter().all(|x| *x) {
            Ok(ActionChange::SwapState(GameState::Done(self.clone())))
        } else {
            Ok(ActionChange::None)
        }
    }

    pub fn make_action(
        &mut self,
        shared: &mut SharedState,
        player_id: u64,
        action: ActionKind,
    ) -> MakeActionResult {
        match action {
            ActionKind::Place(x, y) => self.make_action_place(shared, (x, y)),
            ActionKind::Pass => self.make_action_pass(shared, player_id),
            ActionKind::Cancel => Ok(ActionChange::PopState),
            ActionKind::Resign => self.make_action_resign(shared, player_id),
        }
    }
}

/// Scores a board by filling in fully surrounded empty spaces based on chinese rules
fn score_board(board: &Board, groups: &[Group]) -> Board {
    let &Board {
        width,
        height,
        toroidal,
        ..
    } = board;
    let mut board = Board::empty(width, height, toroidal);

    // Fill living groups to the board
    for group in groups {
        if !group.alive {
            continue;
        }
        for point in &group.points {
            *board.point_mut(*point) = group.team;
        }
    }

    // Find empty points
    let mut legal_points = board
        .points
        .iter()
        .enumerate()
        .filter_map(|(idx, c)| {
            if c.is_empty() {
                board.idx_to_coord(idx)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    #[derive(Copy, Clone)]
    enum SeenTeams {
        Zero,
        One(Color),
        Many,
    }
    use SeenTeams::*;

    let mut seen = HashSet::new();
    let mut stack = VecDeque::new();
    let mut marked = Vec::new();

    while let Some(point) = legal_points.pop() {
        stack.push_back(point);

        let mut collisions = SeenTeams::Zero;

        while let Some(point) = stack.pop_front() {
            marked.push(point);
            for point in board.surrounding_points(point) {
                if !seen.insert(point) {
                    continue;
                }

                match board.get_point(point) {
                    Color(0) => {
                        stack.push_back(point);
                        legal_points.retain(|x| *x != point);
                    }
                    c => {
                        collisions = match collisions {
                            Zero => One(c),
                            One(x) if x == c => One(x),
                            One(_) => Many,
                            Many => Many,
                        }
                    }
                }
            }
        }

        // The floodfill touched only a single color -> this must be their territory
        if let One(color) = collisions {
            for point in marked.drain(..) {
                *board.point_mut(point) = color;
            }
        }

        seen.clear();
        marked.clear();
    }

    board
}
