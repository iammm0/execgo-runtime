use crate::{
    error::{AppError, AppResult},
    types::{ResourceCapacity, RuntimeResourcesResponse, TaskResourceReservation},
};

/// ResourceLedger 维护 runtime 的资源容量、预留和可用量计算 / maintains runtime capacity, reservations, and available-resource calculations.
#[derive(Debug, Clone)]
pub struct ResourceLedger {
    capacity: ResourceCapacity,
}

impl ResourceLedger {
    /// new 使用给定容量创建资源账本 / creates a resource ledger from the given capacity snapshot.
    pub fn new(capacity: ResourceCapacity) -> Self {
        Self { capacity }
    }

    /// capacity 返回账本的总容量快照 / returns the total capacity snapshot of the ledger.
    pub fn capacity(&self) -> &ResourceCapacity {
        &self.capacity
    }

    /// ensure_within_capacity 校验单个任务请求本身不超过 runtime 总容量 / validates that one reservation request does not exceed total runtime capacity.
    pub fn ensure_within_capacity(&self, reservation: &TaskResourceReservation) -> AppResult<()> {
        if reservation.task_slots > self.capacity.task_slots {
            return Err(AppError::InsufficientResources(format!(
                "task requires {} task slots but runtime capacity is {}",
                reservation.task_slots, self.capacity.task_slots
            )));
        }

        if let (Some(requested), Some(capacity)) =
            (reservation.memory_bytes, self.capacity.memory_bytes)
        {
            if requested > capacity {
                return Err(AppError::InsufficientResources(format!(
                    "task requires {requested} memory_bytes but runtime capacity is {capacity}"
                )));
            }
        }

        if let (Some(requested), Some(capacity)) = (reservation.pids, self.capacity.pids) {
            if requested > capacity {
                return Err(AppError::InsufficientResources(format!(
                    "task requires {requested} pids but runtime capacity is {capacity}"
                )));
            }
        }

        Ok(())
    }

    /// can_reserve 判断当前已预留资源上再叠加一个任务是否仍可接受 / reports whether another reservation can be accepted on top of the currently reserved resources.
    pub fn can_reserve(
        &self,
        currently_reserved: &ResourceCapacity,
        reservation: &TaskResourceReservation,
    ) -> bool {
        if currently_reserved
            .task_slots
            .saturating_add(reservation.task_slots)
            > self.capacity.task_slots
        {
            return false;
        }

        if let (Some(reserved), Some(requested), Some(capacity)) = (
            currently_reserved.memory_bytes,
            reservation.memory_bytes,
            self.capacity.memory_bytes,
        ) {
            if reserved.saturating_add(requested) > capacity {
                return false;
            }
        }

        if let (Some(reserved), Some(requested), Some(capacity)) = (
            currently_reserved.pids,
            reservation.pids,
            self.capacity.pids,
        ) {
            if reserved.saturating_add(requested) > capacity {
                return false;
            }
        }

        true
    }

    /// reserved_capacity 根据活动预留列表聚合已占用容量 / aggregates the reserved capacity from active reservations.
    pub fn reserved_capacity<'a, I>(&self, reservations: I) -> ResourceCapacity
    where
        I: IntoIterator<Item = &'a TaskResourceReservation>,
    {
        let mut task_slots = 0u64;
        let mut memory = if self.capacity.memory_bytes.is_some() {
            Some(0u64)
        } else {
            None
        };
        let mut pids = if self.capacity.pids.is_some() {
            Some(0u64)
        } else {
            None
        };

        for reservation in reservations {
            task_slots = task_slots.saturating_add(reservation.task_slots);
            if let Some(value) = reservation.memory_bytes {
                let current = memory.unwrap_or(0);
                memory = Some(current.saturating_add(value));
            }
            if let Some(value) = reservation.pids {
                let current = pids.unwrap_or(0);
                pids = Some(current.saturating_add(value));
            }
        }

        ResourceCapacity {
            task_slots,
            memory_bytes: memory,
            pids,
        }
    }

    /// available_capacity 计算当前剩余可分配容量 / computes the currently available capacity.
    pub fn available_capacity(&self, reserved: &ResourceCapacity) -> ResourceCapacity {
        ResourceCapacity {
            task_slots: self.capacity.task_slots.saturating_sub(reserved.task_slots),
            memory_bytes: self
                .capacity
                .memory_bytes
                .map(|capacity| capacity.saturating_sub(reserved.memory_bytes.unwrap_or(0))),
            pids: self
                .capacity
                .pids
                .map(|capacity| capacity.saturating_sub(reserved.pids.unwrap_or(0))),
        }
    }

    /// empty_snapshot 构造没有活动预留时的资源视图 / builds a resource view with no active reservations.
    pub fn empty_snapshot(&self, runtime_id: String) -> RuntimeResourcesResponse {
        let reserved = self.reserved_capacity(std::iter::empty::<&TaskResourceReservation>());
        RuntimeResourcesResponse {
            runtime_id,
            capacity: self.capacity.clone(),
            available: self.available_capacity(&reserved),
            reserved,
            active_reservations: Vec::new(),
            accepted_waiting_tasks: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_capacity_overflow() {
        let ledger = ResourceLedger::new(ResourceCapacity {
            task_slots: 2,
            memory_bytes: Some(100),
            pids: Some(8),
        });
        let reserved = ResourceCapacity {
            task_slots: 1,
            memory_bytes: Some(50),
            pids: Some(2),
        };
        let reservation = TaskResourceReservation {
            task_slots: 2,
            memory_bytes: Some(60),
            pids: Some(1),
        };
        assert!(!ledger.can_reserve(&reserved, &reservation));
    }
}
