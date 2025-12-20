#pragma once

#include "valhalla/sif/costfactory.h"

#include "rust/cxx.h"

/// Helper method that creates a new DynamicCost object using the CostFactory and also the provided parameters.
inline std::shared_ptr<valhalla::sif::DynamicCost> new_cost(rust::Slice<const uint8_t> raw_costing) {
  valhalla::Costing costing;
  if (!costing.ParseFromArray(raw_costing.data(), raw_costing.size())) {
    throw std::runtime_error("Failed to parse costing options");
  }

  valhalla::sif::CostFactory factory;
  return factory.Create(costing);
}
