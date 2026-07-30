use core::cell::RefCell;
use crate::vga::Writer;

pub struct Task {
    pub id: usize,
    pub name: &'static str,
    pub state: TaskState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Waiting,
    Completed,
}

pub struct Scheduler {
    tasks: RefCell<[Option<Task>; 16]>,
    next_id: RefCell<usize>,
    current: RefCell<Option<usize>>,
}

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            tasks: RefCell::new([None, None, None, None, None, None, None, None, None, None, None, None, None, None, None, None]),
            next_id: RefCell::new(0),
            current: RefCell::new(None),
        }
    }

    pub fn add_task(&self, name: &'static str) -> usize {
        let mut tasks = self.tasks.borrow_mut();
        let mut id = *self.next_id.borrow();
        while id < tasks.len() {
            if tasks[id].is_none() {
                tasks[id] = Some(Task {
                    id,
                    name,
                    state: TaskState::Ready,
                });
                *self.next_id.borrow_mut() = id + 1;
                return id;
            }
            id += 1;
        }
        0
    }

    pub fn schedule(&self) -> Option<usize> {
        let mut tasks = self.tasks.borrow_mut();
        let current = *self.current.borrow();
        let mut next = current.unwrap_or(0);

        for _ in 0..tasks.len() {
            next = (next + 1) % tasks.len();
            if let Some(task) = &tasks[next] {
                if task.state == TaskState::Ready {
                    *self.current.borrow_mut() = Some(next);
                    tasks[next].as_mut().unwrap().state = TaskState::Running;
                    return Some(next);
                }
            }
        }
        None
    }

    pub fn set_state(&self, id: usize, state: TaskState) {
        let mut tasks = self.tasks.borrow_mut();
        if let Some(task) = &mut tasks[id] {
            task.state = state;
        }
    }

    pub fn print_status(&self, writer: &mut Writer) {
        let tasks = self.tasks.borrow();
        writer.write_line("Scheduler status:");
        for task in tasks.iter().flatten() {
            writer.write_string("  task ");
            writer.write_decimal(task.id);
            writer.write_string(": ");
            writer.write_string(task.name);
            writer.write_string(" ");
            writer.write_string(match task.state {
                TaskState::Ready => "Ready",
                TaskState::Running => "Running",
                TaskState::Waiting => "Waiting",
                TaskState::Completed => "Completed",
            });
            writer.write_line("");
        }
    }
}
