use std::{
    collections::{HashMap, HashSet, hash_map::Entry},
    hash::Hash,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct TaskId(u64);

impl TaskId {
    fn next(&mut self) -> Self {
        let out = *self;
        self.0 += 1;
        out
    }
}

pub struct TaskDag<T> {
    id_counter: TaskId,
    frontier: HashSet<TaskId>,
    graph: HashMap<TaskId, Task<T>>,
}

impl<T> Default for TaskDag<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> TaskDag<T> {
    pub fn new() -> Self {
        Self {
            id_counter: TaskId(0),
            frontier: HashSet::new(),
            graph: HashMap::new(),
        }
    }

    /// Add task to DAG and returns the TaskId of the task
    pub fn add_task(&mut self, task: T) -> TaskId {
        let id = self.id_counter.next();
        self.frontier.insert(id);
        self.graph.insert(
            id,
            Task {
                dependencies: HashSet::new(),
                dependents: HashSet::new(),
                value: task,
            },
        );

        id
    }

    /// Requires both TaskId to be valid tasks
    pub fn set_dependency(&mut self, dependent: TaskId, dependency: TaskId) {
        if dependent == dependency {
            return;
        }

        self.graph
            .get_mut(&dependent)
            .expect(&format!("dependent={dependent:?} is invalid"))
            .dependencies
            .insert(dependency);

        self.graph
            .get_mut(&dependency)
            .expect(&format!("dependency={dependency:?} is invalid"))
            .dependents
            .insert(dependent);

        self.frontier.remove(&dependent);
    }

    /// Mark task as done, removing task from the DAG
    pub fn mark_done(&mut self, task_id: TaskId) {
        self.frontier.remove(&task_id);
        if let Some(task) = self.graph.remove(&task_id) {
            for dependency in task.dependencies {
                if let Some(task) = self.graph.get_mut(&dependency) {
                    task.dependents.remove(&task_id);
                }
            }

            for dependent in task.dependents {
                if let Some(task) = self.graph.get_mut(&dependent) {
                    task.dependencies.remove(&task_id);
                    if task.dependencies.is_empty() {
                        self.frontier.insert(dependent);
                    }
                }
            }
        }
    }

    pub fn get(&self, task_id: &TaskId) -> Option<&T> {
        Some(&self.graph.get(task_id)?.value)
    }

    /// Return set of tasks that has no pending dependencies
    pub fn doables(&self) -> &HashSet<TaskId> {
        &self.frontier
    }

    /// Merge two tasks into one tasks with the union of dependencies and dependents
    /// Keeping the into task and removing the from task
    pub fn merge_tasks(&mut self, from: TaskId, into: TaskId) {
        let from_task = self
            .graph
            .get(&from)
            .expect("merging task but from is not valid");
        let dependencies = from_task.dependencies.clone();
        let dependents = from_task.dependents.clone();

        self.mark_done(from);

        for dependency in dependencies {
            self.set_dependency(into, dependency);
        }

        for dependent in dependents {
            self.set_dependency(dependent, into);
        }
    }
}

/// TaskDag with properties:
/// - If multiple copies of the same task is queued (parked), squash them and do only once
/// - Same task does not run overlapped
pub struct DedupedTaskDag<T: Hash + Eq + Clone> {
    task_dag: TaskDag<T>,

    // rules of deduping gurantee there are no duplicates
    parked: HashMap<T, TaskId>,
    running: HashMap<T, TaskId>,
}

impl<T: Hash + Eq + Clone> DedupedTaskDag<T> {
    pub fn new() -> Self {
        Self {
            task_dag: TaskDag::new(),
            parked: HashMap::new(),
            running: HashMap::new(),
        }
    }

    fn add_task(&mut self, task: T) -> TaskId {
        if let Some(parked) = self.parked.get(&task) {
            return *parked;
        }

        let id = self.task_dag.add_task(task.clone());

        if let Some(running) = self.running.get(&task) {
            self.task_dag.set_dependency(id, *running);
        }

        self.parked.insert(task, id);
        id
    }

    /// Requires dependent to be parked for deduplication to function
    pub fn add_with_dependencies(&mut self, dependent: T, dependencies: Vec<T>) {
        let dependent_id = *self
            .parked
            .get(&dependent)
            .expect("dependent is not parked");

        for dependency in dependencies {
            let dependency_id = self.add_task(dependency);
            self.task_dag.set_dependency(dependent_id, dependency_id);
        }
    }

    pub fn mark_parked(&mut self, task: T) {
        let id = self.running.remove(&task).expect("task is not running");

        match self.parked.entry(task) {
            Entry::Occupied(existing) => {
                let existing = *existing.get();
                self.task_dag.merge_tasks(id, existing);
            }
            Entry::Vacant(entry) => {
                entry.insert(id);
            }
        }
    }

    pub fn mark_running(&mut self, task: T) {
        let id = self.parked.remove(&task).expect("task is not parked");

        match self.running.entry(task) {
            Entry::Occupied(_) => {
                panic!("an existing copy of the task is already running")
            }
            Entry::Vacant(entry) => {
                entry.insert(id);
            }
        }
    }
}

struct Task<T> {
    dependencies: HashSet<TaskId>,
    dependents: HashSet<TaskId>,
    value: T,
}
